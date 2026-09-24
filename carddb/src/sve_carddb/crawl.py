"""Crawl stages for the Japanese official site.

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

from sve_carddb.fetch.client import FetchError, Request
from sve_carddb.fetch.validate import ValidationError, require_media_type
from sve_carddb.fetch.writer import Fetched, LocalState, sha256
from sve_carddb.html import MissingElementError
from sve_carddb.manifest import Kind, Link, Outcome, RequestResult
from sve_carddb.sources import official_jp as jp

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import PurePosixPath

    from sve_carddb.fetch.client import Client, Response
    from sve_carddb.fetch.throttle import CircuitBreaker
    from sve_carddb.fetch.writer import Writer
    from sve_carddb.manifest import Manifest

SETS_ROOT = f"{jp.REGION.value}:sets"
_OK = 200
_NOT_MODIFIED = 304
_PAGE_ERRORS = (ValidationError, MissingElementError)


class Mode(StrEnum):
    RESUME = "resume"
    REFRESH = "refresh"
    REPAIR = "repair"


class LimitReachedError(RuntimeError):
    """The run fetched `--limit` URLs; stop cleanly."""


class ListInconsistentError(RuntimeError):
    """A product list changed while it was being read, on every attempt."""


def list_root(set_code: str) -> str:
    """The generation root of a product's list."""
    return f"{jp.REGION.value}:list:{set_code}"


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
    fetched: int = field(default=0, init=False)

    async def page[T](
        self,
        url: str,
        *,
        kind: Kind,
        path: PurePosixPath,
        parse: Callable[[bytes], T],
        bypass_resume: bool = False,
    ) -> Page[T]:
        """Return the parsed page, fetching it only when the mode requires.

        resume skips trusted copies (unless `bypass_resume`); refresh
        re-checks them; repair only fetches copies that are not trusted.
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
            require_media_type(response.content_type, "text/html")
            value = parse(response.body)
        except _PAGE_ERRORS as exc:
            self._fail(response, str(exc))
        self.writer.write(
            Fetched(
                url=url,
                region=jp.REGION,
                kind=kind,
                path=path,
                body=response.body,
                content_type=response.content_type or "",
                etag=response.etag,
                last_modified=response.last_modified,
                compressed=True,
            ),
            request_id=response.request_id,
            rewrite=self.mode is Mode.REPAIR,
        )
        self.breaker.record_success()
        return Page(value, sha256(response.body))

    async def _fetch(
        self, url: str, path: PurePosixPath, *, conditional: bool
    ) -> Response:
        if self.limit is not None and self.fetched >= self.limit:
            msg = f"--limit of {self.limit} URLs reached"
            raise LimitReachedError(msg)
        self.fetched += 1
        self.writer.check_path(url, path)
        resource = self.manifest.resources.get(url) if conditional else None
        etag = resource.etag if resource is not None else None
        response = await self.client.get(
            Request(url=url, allowed=jp.allowed, if_none_match=etag)
        )
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
        generation = self.manifest.generations.start(SETS_ROOT)
        url = jp.sets_url()
        page = await self.page(
            url,
            kind=Kind.SETS,
            path=jp.sets_path(),
            parse=jp.parse_sets,
            bypass_resume=True,
        )
        links = [
            Link(jp.list_url(s.code, 1), Kind.LIST, i, s.code)
            for i, s in enumerate(page.value)
        ]
        self._add_generation_page(generation.id, url, Page(links, page.sha256))
        self.manifest.generations.validate(
            generation.id, declared_total=len(page.value)
        )
        return page.value

    def current_sets(self) -> list[str]:
        """Product codes from the validated product generation."""
        current = self.manifest.generations.current(SETS_ROOT)
        if current is None:
            return []
        return [
            edge.link.original for edge in self.manifest.generations.edges(current.id)
        ]

    async def first_page(self, set_code: str) -> ListSummary:
        """P0: page 1 of a product list and the totals it declares."""
        page = await self.page(
            jp.list_url(set_code, 1),
            kind=Kind.LIST,
            path=jp.list_path(set_code, 1),
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
        generation = self.manifest.generations.start(list_root(set_code))
        first_url = jp.list_url(set_code, 1)
        first = await self._list_page(first_url, set_code, 1, jp.parse_list_first)
        summary = _summary(set_code, first.value)
        numbers = list(first.value.card_numbers)
        self._record_list_page(generation.id, first_url, first)
        for page_no in range(2, summary.max_page + 1):
            url = jp.list_url(set_code, page_no)
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
            path=jp.list_path(set_code, page_no),
            parse=parse,
            bypass_resume=True,
        )

    def _record_list_page(
        self, generation_id: int, url: str, page: Page[jp.ListPage]
    ) -> None:
        links = [
            Link(jp.card_url(n), Kind.CARD, i, n)
            for i, n in enumerate(page.value.card_numbers)
        ]
        self._add_generation_page(generation_id, url, Page(links, page.sha256))

    # --- P2 ---------------------------------------------------------------

    def card_numbers(self, set_codes: list[str] | None = None) -> list[str]:
        """Card numbers from validated list generations, deduplicated, in list order."""
        numbers: dict[str, None] = {}
        for code in set_codes if set_codes is not None else self.current_sets():
            current = self.manifest.generations.current(list_root(code))
            if current is None:
                continue
            for edge in self.manifest.generations.edges(current.id):
                numbers.setdefault(edge.link.original)
        return list(numbers)

    async def card(self, number: str) -> jp.CardPage:
        """Fetch one card page and record its images as the card's current links."""
        url = jp.card_url(number)
        page = await self.page(
            url,
            kind=Kind.CARD,
            path=jp.card_path(number),
            parse=lambda body: jp.parse_card(body, expected_number=number),
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


def _summary(set_code: str, page: jp.ListPage) -> ListSummary:
    if page.total is None or page.max_page is None:
        msg = f"{set_code}: page 1 has no totals"
        raise ValidationError(msg)
    return ListSummary(set_code, page.total, page.max_page)


def _more(page: int, max_page: int, total: int) -> Callable[[bytes], jp.ListPage]:
    return lambda body: jp.parse_list_more(
        body, page=page, max_page=max_page, total=total
    )
