"""Protected incremental Q&A fetching; transport is injectable for offline tests."""

import asyncio
import time
from pathlib import PurePosixPath
from typing import TYPE_CHECKING, Literal
from urllib.parse import urljoin

from sve_carddb.card_extras.archive import card_number
from sve_carddb.card_extras.generation import (
    Closure,
    Observation,
    closure,
    links,
    root_key,
)
from sve_carddb.crawl import EN_CATALOG, JP_CATALOG, Crawler, Mode, Site
from sve_carddb.fetch.client import Client
from sve_carddb.fetch.refresh import RefreshWriter
from sve_carddb.fetch.throttle import CircuitBreaker, Throttle
from sve_carddb.fetch.validate import decode_html
from sve_carddb.html import attribute, parse, select_all
from sve_carddb.manifest import GenerationStatus, Kind, Link
from sve_carddb.manifest import Region as ManifestRegion
from sve_carddb.snapshot.values import digest
from sve_carddb.sources.official_qa import allowed, parse_qa
from sve_carddb.urls import canonicalize

if TYPE_CHECKING:
    from collections.abc import Awaitable, Callable

    import httpx

    from sve_carddb.registry.records import Region


class QACrawler:
    def __init__(
        self,
        http: httpx.AsyncClient,
        writer: RefreshWriter,
        *,
        region: Region,
        run_id: str,
        clock: Callable[[], float] = time.monotonic,
        sleep: Callable[[float], Awaitable[None]] = asyncio.sleep,
    ) -> None:
        if not isinstance(writer, RefreshWriter):
            raise TypeError("Q&A fetching requires the protected refresh writer")
        self.writer = writer
        self.region = region
        manifest = writer.manifest
        site = Site(
            ManifestRegion(region),
            lambda url: allowed(url, region),
            self.path,
        )
        client = Client(
            http,
            Throttle(2.0, 0.0, clock=clock, sleep=sleep),
            manifest,
            run_id=run_id,
            clock=clock,
        )
        self.crawler = Crawler(
            client, writer, manifest, CircuitBreaker(3), Mode.REFRESH, site=site
        )
        self.observations: dict[str, Observation] = {}

    def path(self, url: str) -> PurePosixPath:
        """Avoid URL/path collisions while preserving exact URLs in the manifest."""
        return PurePosixPath(
            "raw", self.region, "qa", digest(url.encode())[7:] + ".html.zst"
        )

    async def _page(
        self, url: str, kind: Literal["index", "detail"], generation: int
    ) -> None:
        if url in self.observations:
            return
        page = await self.crawler.page(
            url,
            kind=Kind.LIST if kind == "index" else Kind.QA,
            path=self.path(url),
            parse=lambda raw: parse_qa(raw, url=url, region=self.region, kind=kind),
        )
        edges = links(page.value)
        with self.writer.manifest.transaction():
            self.writer.manifest.links.replace(url, page.sha256, edges)
            self.writer.manifest.generations.add_page(
                generation, url, page.sha256, edges
            )
        self.observations[url] = Observation(page.value, page.sha256)

    async def collect(self, root: str) -> Closure:
        """Retain partial observations; validate only closed, unchanged discovery."""
        if not allowed(root, self.region) or canonicalize(root) != root:
            raise ValueError("Q&A root must be a canonical regional HTTPS URL")
        generations = self.writer.manifest.generations
        generation = generations.start(root_key(root, self.region))
        self.observations = {}
        try:
            await self._page(root, "index", generation.id)
            first = self.observations[root]
            assert first.parsed.pagination is not None
            for _, url in first.parsed.pagination.urls:
                await self._page(url, "index", generation.id)
            for observed in tuple(self.observations.values()):
                for detail in observed.parsed.details:
                    await self._page(detail.url, "detail", generation.id)
            checked = closure(root, self.observations)
            if checked.complete:
                recheck = await self.crawler.page(
                    root,
                    kind=Kind.LIST,
                    path=self.path(root),
                    parse=lambda raw: parse_qa(
                        raw, url=root, region=self.region, kind="index"
                    ),
                )
                if recheck.sha256 != first.sha256:
                    checked = Closure(
                        ("root_changed",), checked.total, checked.max_page
                    )
            if checked.complete:
                generations.validate(
                    generation.id,
                    declared_total=checked.total,
                    max_page=checked.max_page,
                )
            else:
                generations.fail(generation.id)
            return checked
        finally:
            if generations.get(generation.id).status is GenerationStatus.IN_PROGRESS:
                generations.fail(generation.id)
            self.writer.checkpoint()

    async def cards(self, numbers: tuple[str, ...]) -> tuple[str, ...]:
        """Refresh explicit physical card pages through the same protected transport."""
        catalog = JP_CATALOG if self.region == "jp" else EN_CATALOG
        site = Site(
            catalog.region,
            lambda url: card_number(url, self.region) is not None,
            catalog.site.image_path,
            self.crawler.site.headers,
        )
        crawler = Crawler(
            self.crawler.client,
            self.writer,
            self.writer.manifest,
            CircuitBreaker(3),
            Mode.REFRESH,
            site=site,
            catalog=catalog,
        )
        urls: list[str] = []
        try:
            for number in dict.fromkeys(numbers):
                await crawler.card(number)
                url = catalog.card_url(number)
                raw = self.writer.read(url)
                related = [
                    Link(urljoin(url, href), Kind.CARD, position, href)
                    for position, node in enumerate(
                        select_all(
                            parse(decode_html(raw, min_bytes=1)),
                            ".cardlist-Detail_Relation a",
                        )
                    )
                    if (href := attribute(node, "href")) is not None
                ]
                resource = self.writer.manifest.resources.get(url)
                assert resource is not None
                with self.writer.manifest.transaction():
                    self.writer.manifest.links.replace(
                        url,
                        resource.sha256,
                        [*self.writer.manifest.links.current(url), *related],
                    )
                urls.append(url)
            return tuple(urls)
        finally:
            self.writer.checkpoint()
