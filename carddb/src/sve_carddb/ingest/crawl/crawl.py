"""Crawl stages for the Japanese and English official sites.

P0 discovers products and fetches page 1 of each list. P1 discovers every list
page as one generation per product. P2 fetches the card pages listed by the
current validated generations. Scheduling only ever reads validated
generations, so a half-finished discovery never drives downstream fetching.

Generation pages and links are recorded right after each page, whether it was
fetched or read from a trusted local copy. A crash in between is safe: a
generation missing a page never validates, and links are re-recorded from the
local copy on the next run.
"""

from dataclasses import dataclass, field
from enum import StrEnum
from typing import TYPE_CHECKING, NoReturn

from sve_carddb.ingest.archive.manifest import Kind, Link, Outcome, RequestResult
from sve_carddb.ingest.http.client import FetchError, Request
from sve_carddb.ingest.http.validate import (
    ValidationError,
    check_image,
    require_media_type,
)
from sve_carddb.ingest.http.writer import Fetched, LocalState, sha256
from sve_carddb.ingest.queries import card_numbers, current_sets, list_root, sets_root
from sve_carddb.parse.html import MissingElementError
from sve_carddb.parse.pages import official_en as en
from sve_carddb.parse.pages import official_jp as jp

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import PurePosixPath

    from sve_carddb.core.regions import SourceRegion as Region
    from sve_carddb.ingest.archive.manifest import Manifest
    from sve_carddb.ingest.http.client import Client, Response
    from sve_carddb.ingest.http.throttle import CircuitBreaker
    from sve_carddb.ingest.http.writer import Writer

_OK = 200
_NOT_MODIFIED = 304
_NOT_FOUND = 404
_PAGE_ERRORS = (ValidationError, MissingElementError)
# The largest card image seen is about 2 MB; anything far above is not a card image.
IMAGE_MAX_BYTES = 20 * 1024 * 1024


class Mode(StrEnum):
    RESUME = "resume"
    REFRESH = "refresh"
    REPAIR = "repair"


@dataclass(frozen=True, slots=True)
class Site:
    """What the crawler needs to know about one source site."""

    region: Region
    allowed: Callable[[str], bool]
    image_path: Callable[[str], PurePosixPath]
    headers: Callable[[str], tuple[tuple[str, str], ...]] = lambda _url: ()


JP_SITE = Site(jp.REGION, jp.allowed, jp.image_path)
EN_SITE = Site(en.REGION, en.allowed, en.image_path)


@dataclass(frozen=True, slots=True)
class Catalog:
    """Where one official card list lives: its URLs, local paths and card parser.

    Both sites share the product form and list markup, so only these differ.
    """

    site: Site
    sets_url: Callable[[], str]
    sets_path: Callable[[], PurePosixPath]
    list_url: Callable[[str, int], str]
    list_path: Callable[[str, int], PurePosixPath]
    card_url: Callable[[str], str]
    card_path: Callable[[str], PurePosixPath]
    parse_card: Callable[[bytes, str], jp.CardPage]

    @property
    def region(self) -> Region:
        """The region the stored pages are recorded under."""
        return self.site.region


JP_CATALOG = Catalog(
    JP_SITE,
    jp.sets_url,
    jp.sets_path,
    jp.list_url,
    jp.list_path,
    jp.card_url,
    jp.card_path,
    lambda body, number: jp.parse_card(body, expected_number=number),
)
EN_CATALOG = Catalog(
    EN_SITE,
    en.sets_url,
    en.sets_path,
    en.list_url,
    en.list_path,
    en.card_url,
    en.card_path,
    lambda body, number: en.parse_card(body, expected_number=number),
)


class LimitReachedError(RuntimeError):
    """The run fetched `--limit` URLs; stop cleanly."""


class ListInconsistentError(RuntimeError):
    """A product list changed while it was being read, on every attempt."""


def image_urls(
    manifest: Manifest,
    set_codes: list[str] | None = None,
    catalog: Catalog = JP_CATALOG,
) -> list[str]:
    """Card image URLs recorded by P2, deduplicated, in card and page order."""
    urls: dict[str, None] = {}
    for number in card_numbers(manifest, set_codes, catalog.region):
        for link in manifest.links.current(catalog.card_url(number)):
            if link.to_kind is Kind.IMAGE:
                urls.setdefault(link.to_url)
    return list(urls)


@dataclass(frozen=True, slots=True)
class Page[T]:
    """A parsed page and the hash of the content it was parsed from."""

    value: T
    sha256: str


@dataclass(frozen=True, slots=True)
class ListSummary:
    """What page 1 of a product list declares."""

    set_code: str
    total: int
    max_page: int


@dataclass
class Crawler:
    """Fetches pages through the client, validates them and stores them."""

    client: Client
    writer: Writer
    manifest: Manifest
    breaker: CircuitBreaker
    mode: Mode = Mode.RESUME
    limit: int | None = None
    site: Site = JP_SITE
    # P0-P2 use the catalog; `site` must be `catalog.site` when they run.
    catalog: Catalog = JP_CATALOG
    fetched: int = field(default=0, init=False)

    async def page[T](
        self,
        url: str,
        *,
        kind: Kind,
        path: PurePosixPath,
        parse: Callable[[bytes], T],
        bypass_resume: bool = False,
        media_type: str = "text/html",
    ) -> Page[T]:
        """Return the parsed page, fetching it only when the mode requires.

        resume skips trusted copies (unless `bypass_resume`); refresh
        re-checks them; repair only fetches copies that are not trusted.
        The copy is compressed when `path` ends in `.zst`.
        """
        state = self.writer.local_state(url)
        skip = self.mode is Mode.REPAIR or (
            self.mode is Mode.RESUME and not bypass_resume
        )
        if state is LocalState.TRUSTED and skip:
            body = self.writer.read(url)
            return Page(parse(body), sha256(body))
        response = await self._fetch(url, path, conditional=state is LocalState.TRUSTED)
        if response.status == _NOT_MODIFIED:
            self.writer.mark_not_modified(url, request_id=response.request_id)
            self.breaker.record_success()
            body = self.writer.read(url)
            return Page(parse(body), sha256(body))
        try:
            require_media_type(response.content_type, media_type)
            value = parse(response.body)
        except _PAGE_ERRORS as exc:
            self._fail(response, str(exc))
        self.writer.write(
            Fetched(
                url=url,
                region=self.site.region,
                kind=kind,
                path=path,
                body=response.body,
                content_type=response.content_type or "",
                etag=response.etag,
                last_modified=response.last_modified,
                compressed=path.suffix == ".zst",
            ),
            request_id=response.request_id,
            rewrite=self.mode is Mode.REPAIR,
        )
        self.breaker.record_success()
        return Page(value, sha256(response.body))

    async def _fetch(
        self,
        url: str,
        path: PurePosixPath,
        *,
        conditional: bool,
        missing_ok: bool = False,
    ) -> Response:
        """Send one logical request; a missing resource is returned only when `missing_ok`."""
        if self.limit is not None and self.fetched >= self.limit:
            msg = f"--limit of {self.limit} URLs reached"
            raise LimitReachedError(msg)
        self.fetched += 1
        self.writer.check_path(url, path)
        resource = self.manifest.resources.get(url) if conditional else None
        etag = resource.etag if resource is not None else None
        response = await self.client.get(
            Request(
                url=url,
                allowed=self.site.allowed,
                if_none_match=etag,
                headers=self.site.headers(url),
            )
        )
        if missing_ok and _missing(response):
            with self.manifest.transaction():
                self.manifest.requests.finish(
                    response.request_id,
                    RequestResult(
                        outcome=Outcome.FAILED,
                        final_url=response.url,
                        status=response.status,
                        error_class="not_found",
                    ),
                )
            return response
        if response.status == _NOT_MODIFIED and etag is None:
            self._fail(response, "304 without a conditional request")
        if response.status not in {_OK, _NOT_MODIFIED}:
            self._fail(response, f"unexpected HTTP {response.status}")
        return response

    def _fail(self, response: Response, reason: str) -> NoReturn:
        with self.manifest.transaction():
            self.manifest.requests.finish(
                response.request_id,
                RequestResult(
                    outcome=Outcome.FAILED,
                    final_url=response.url,
                    status=response.status,
                    error_class="validation",
                    validation_error=reason,
                ),
            )
        self.breaker.record_failure(reason)
        msg = f"{response.url}: {reason}"
        raise FetchError(msg)

    def _add_generation_page(
        self, generation_id: int, url: str, page: Page[list[Link]]
    ) -> None:
        with self.manifest.transaction():
            self.manifest.generations.add_page(
                generation_id, url, page.sha256, page.value
            )

    # --- P0 ---------------------------------------------------------------

    async def discover_sets(self) -> list[jp.CardSet]:
        """Fetch the product list as a one-page generation and publish it."""
        generation = self.manifest.generations.start(sets_root(self.catalog.region))
        url = self.catalog.sets_url()
        page = await self.page(
            url,
            kind=Kind.SETS,
            path=self.catalog.sets_path(),
            parse=jp.parse_sets,
            bypass_resume=True,
        )
        links = [
            Link(self.catalog.list_url(s.code, 1), Kind.LIST, i, s.code)
            for i, s in enumerate(page.value)
        ]
        self._add_generation_page(generation.id, url, Page(links, page.sha256))
        self.manifest.generations.validate(
            generation.id, declared_total=len(page.value)
        )
        return page.value

    def current_sets(self) -> list[str]:
        """Product codes from the validated product generation."""
        return current_sets(self.manifest, self.catalog.region)

    async def first_page(self, set_code: str) -> ListSummary:
        """P0: page 1 of a product list and the totals it declares."""
        page = await self.page(
            self.catalog.list_url(set_code, 1),
            kind=Kind.LIST,
            path=self.catalog.list_path(set_code, 1),
            parse=jp.parse_list_first,
        )
        return _summary(set_code, page.value)

    # --- P1 ---------------------------------------------------------------

    async def discover_list(self, set_code: str, *, attempts: int = 2) -> ListSummary:
        """Read a whole product list as one generation; start over if it shifts."""
        for _ in range(attempts):
            summary = await self._try_discover_list(set_code)
            if summary is not None:
                return summary
        msg = f"{set_code}: list changed while reading it, {attempts} times"
        raise ListInconsistentError(msg)

    async def _try_discover_list(self, set_code: str) -> ListSummary | None:
        generation = self.manifest.generations.start(
            list_root(set_code, self.catalog.region)
        )
        first_url = self.catalog.list_url(set_code, 1)
        first = await self._list_page(first_url, set_code, 1, jp.parse_list_first)
        summary = _summary(set_code, first.value)
        numbers = list(first.value.card_numbers)
        self._record_list_page(generation.id, first_url, first)
        for page_no in range(2, summary.max_page + 1):
            url = self.catalog.list_url(set_code, page_no)
            page = await self._list_page(
                url, set_code, page_no, _more(page_no, summary.max_page, summary.total)
            )
            numbers.extend(page.value.card_numbers)
            self._record_list_page(generation.id, url, page)
        # The recheck is an observation only: it is not added to the generation.
        recheck = await self._list_page(first_url, set_code, 1, jp.parse_list_first)
        consistent = (
            len(numbers) == len(set(numbers)) == summary.total
            and _summary(set_code, recheck.value) == summary
            and set(recheck.value.card_numbers) == set(first.value.card_numbers)
        )
        if not consistent:
            self.manifest.generations.fail(generation.id)
            return None
        self.manifest.generations.validate(
            generation.id, declared_total=summary.total, max_page=summary.max_page
        )
        return summary

    async def _list_page(
        self,
        url: str,
        set_code: str,
        page_no: int,
        parse: Callable[[bytes], jp.ListPage],
    ) -> Page[jp.ListPage]:
        return await self.page(
            url,
            kind=Kind.LIST,
            path=self.catalog.list_path(set_code, page_no),
            parse=parse,
            bypass_resume=True,
        )

    def _record_list_page(
        self, generation_id: int, url: str, page: Page[jp.ListPage]
    ) -> None:
        links = [
            Link(self.catalog.card_url(n), Kind.CARD, i, n)
            for i, n in enumerate(page.value.card_numbers)
        ]
        self._add_generation_page(generation_id, url, Page(links, page.sha256))

    # --- P2 ---------------------------------------------------------------

    async def image(self, url: str, *, missing_ok: bool = False) -> bool:
        """Fetch one card image unless the mode lets a trusted copy stand.

        Images are stored as downloaded (PNG and JPEG are compressed already)
        and only structurally checked; decoding is left to thumbnail generation.
        Returns False only for a missing image with `missing_ok`; the caller
        decides whether that counts as a failure.
        """
        state = self.writer.local_state(url)
        if state is LocalState.TRUSTED and self.mode is not Mode.REFRESH:
            return True
        path = self.site.image_path(url)
        response = await self._fetch(
            url, path, conditional=state is LocalState.TRUSTED, missing_ok=missing_ok
        )
        if missing_ok and _missing(response):
            return False
        if response.status == _NOT_MODIFIED:
            self.writer.mark_not_modified(url, request_id=response.request_id)
            self.breaker.record_success()
            return True
        try:
            require_media_type(response.content_type, "image/")
            check_image(response.body, max_bytes=IMAGE_MAX_BYTES)
        except ValidationError as exc:
            self._fail(response, str(exc))
        self.writer.write(
            Fetched(
                url=url,
                region=self.site.region,
                kind=Kind.IMAGE,
                path=path,
                body=response.body,
                content_type=response.content_type or "",
                etag=response.etag,
                last_modified=response.last_modified,
                compressed=False,
            ),
            request_id=response.request_id,
            rewrite=self.mode is Mode.REPAIR,
        )
        self.breaker.record_success()
        return True

    def card_numbers(self, set_codes: list[str] | None = None) -> list[str]:
        """Card numbers from validated list generations, deduplicated, in list order."""
        return card_numbers(self.manifest, set_codes, self.catalog.region)

    async def card(self, number: str) -> jp.CardPage:
        """Fetch one card page and record its images as the card's current links."""
        url = self.catalog.card_url(number)
        page = await self.page(
            url,
            kind=Kind.CARD,
            path=self.catalog.card_path(number),
            parse=lambda body: self.catalog.parse_card(body, number),
        )
        card = page.value
        links = [
            Link(image, Kind.IMAGE, i, original)
            for i, (image, original) in enumerate(
                zip(card.image_urls, card.image_originals, strict=True)
            )
        ]
        with self.manifest.transaction():
            self.manifest.links.replace(url, page.sha256, links)
        return card


def _missing(response: Response) -> bool:
    # shadowverse-portal.com answers a missing image with a redirect to an HTML page.
    media = (response.content_type or "").split(";", 1)[0].strip().lower()
    return response.status == _NOT_FOUND or (
        response.status == _OK and media == "text/html"
    )


def _summary(set_code: str, page: jp.ListPage) -> ListSummary:
    if page.total is None or page.max_page is None:
        msg = f"{set_code}: page 1 has no totals"
        raise ValidationError(msg)
    return ListSummary(set_code, page.total, page.max_page)


def _more(page: int, max_page: int, total: int) -> Callable[[bytes], jp.ListPage]:
    return lambda body: jp.parse_list_more(
        body, page=page, max_page=max_page, total=total
    )
