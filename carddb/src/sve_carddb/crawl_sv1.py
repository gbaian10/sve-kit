"""Crawl stages for shadowverse-portal.com, sharing the JP crawler's machinery.

sv1-cards stores the card API once per language. sv1-images fetches each
card's images from the URL template. When the template misses, it stores the
card page, records its images as the page's links and fetches the image the
page shows instead; a card whose page shows no such image has none.
"""

from dataclasses import dataclass
from enum import StrEnum
from typing import TYPE_CHECKING

from sve_carddb.crawl import Site
from sve_carddb.fetch.client import FetchError
from sve_carddb.fetch.writer import LocalState
from sve_carddb.manifest import Kind, Link
from sve_carddb.sources import official_sv1 as sv1

if TYPE_CHECKING:
    from sve_carddb.crawl import Crawler
    from sve_carddb.fetch.throttle import CircuitBreaker
    from sve_carddb.fetch.writer import Writer
    from sve_carddb.manifest import Manifest

SV1_SITE = Site(sv1.REGION, sv1.allowed, sv1.image_path)


class ImageResult(StrEnum):
    STORED = "stored"
    """A trusted copy already existed; nothing was sent."""
    FETCHED = "fetched"
    NO_IMAGE = "no_image"
    """Neither the template nor the card page has this image."""
    SHARED = "shared"
    """The card page shows another card's image, stored now or earlier."""


def stored_cards(writer: Writer) -> list[sv1.ApiCard] | None:
    """Cards from the stored Japanese API response, or None before `sv1-cards`."""
    url = sv1.api_url("ja")
    if writer.local_state(url) is not LocalState.TRUSTED:
        return None
    return sv1.parse_cards(writer.read(url))


def image_jobs(cards: list[sv1.ApiCard]) -> list[tuple[int, sv1.Face]]:
    """Every image to fetch, by card_id.

    The API lists tokens first, many of which have no image; by card_id,
    regular cards (1xxxxxxxx) come first and tokens (8xxxxxxxx, 9xxxxxxxx) last.
    """
    ordered = sorted(cards, key=lambda card: card.card_id)
    return [(card.card_id, face) for card in ordered for face in card.faces]


def stored_image(
    manifest: Manifest, writer: Writer, card_id: int, face: sv1.Face
) -> bool:
    """True if the image is trusted locally, at the template URL or a fallback."""
    if writer.local_state(sv1.image_url(card_id, face)) is LocalState.TRUSTED:
        return True
    fallback = _linked_image(manifest, card_id, face)
    return fallback is not None and writer.local_state(fallback) is LocalState.TRUSTED


def _linked_image(manifest: Manifest, card_id: int, face: sv1.Face) -> str | None:
    position = sv1.FACES.index(face)
    for link in manifest.links.current(sv1.card_url(card_id)):
        if link.to_kind is Kind.IMAGE and link.position == position:
            return link.to_url
    return None


@dataclass
class Sv1Crawler:
    """Drives a `Crawler` built with `SV1_SITE`.

    `misses` counts, in a row, template misses whose card page shows the image
    elsewhere. The card page resets the main breaker, so without it a changed
    template would cost three requests per image instead of stopping the run.
    """

    crawler: Crawler
    misses: CircuitBreaker

    async def cards(self, lang: str) -> int:
        """Fetch (or re-check with ETag) one language of the card API."""
        page = await self.crawler.page(
            sv1.api_url(lang),
            kind=Kind.API,
            path=sv1.api_path(lang),
            parse=sv1.parse_cards,
            bypass_resume=True,
            media_type="application/json",
        )
        return len(page.value)

    async def image(self, card_id: int, face: sv1.Face) -> ImageResult:
        """Fetch one image unless a trusted copy exists.

        A card without the image is retried from the template on every run
        (one request); its stored card page is not fetched again.
        """
        crawler = self.crawler
        if stored_image(crawler.manifest, crawler.writer, card_id, face):
            return ImageResult.STORED
        url = sv1.image_url(card_id, face)
        if await crawler.image(url, missing_ok=True):
            self.misses.record_success()
            return ImageResult.FETCHED
        found = await self._from_card_page(card_id, face)
        if found is None:
            return ImageResult.NO_IMAGE
        if found == url:
            msg = f"{url}: missing, yet the card page links it"
            raise FetchError(msg)
        if crawler.writer.local_state(found) is LocalState.TRUSTED:
            self.misses.record_success()
            return ImageResult.SHARED
        if sv1.is_template(found, face):
            # Reprints such as 810xxxxxx reuse another card's art, sometimes a card
            # later in the run; a changed template would not match the old shape.
            self.misses.record_success()
            await crawler.image(found)
            return ImageResult.SHARED
        self.misses.record_failure(f"{url}: missing, card page shows {found}")
        await crawler.image(found)
        return ImageResult.FETCHED

    async def _from_card_page(self, card_id: int, face: sv1.Face) -> str | None:
        crawler = self.crawler
        page_url = sv1.card_url(card_id)
        page = await crawler.page(
            page_url,
            kind=Kind.CARD,
            path=sv1.card_path(card_id),
            parse=sv1.parse_card_images,
        )
        images = page.value
        links = [
            Link(url, Kind.IMAGE, i, original)
            for i, (url, original) in enumerate(
                zip(images.urls, images.originals, strict=True)
            )
        ]
        with crawler.manifest.transaction():
            crawler.manifest.links.replace(page_url, page.sha256, links)
        position = sv1.FACES.index(face)
        return images.urls[position] if position < len(images.urls) else None
