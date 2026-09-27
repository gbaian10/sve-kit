"""Crawl stage for shadowverse-wb.com, sharing the JP crawler's machinery.

svwb-cards stores every page of the card list once per language, text only.
"""

from typing import TYPE_CHECKING

from sve_carddb.crawl import Site
from sve_carddb.fetch.client import FetchError
from sve_carddb.manifest import Kind
from sve_carddb.sources import official_svwb as svwb

if TYPE_CHECKING:
    from sve_carddb.crawl import Crawler

SVWB_SITE = Site(svwb.REGION, svwb.allowed, svwb.image_path, svwb.headers)


async def cards(crawler: Crawler, lang: str) -> int:
    """Fetch (or re-check with ETag) every list page of one language.

    Pages are addressed by offset, so a card added mid-run would shift the
    rest; the total must agree on every page and the pages must add up to it.
    """
    count: int | None = None
    seen: set[int] = set()
    offset = 0
    while count is None or offset < count:
        page = await crawler.page(
            svwb.list_url(lang, offset),
            kind=Kind.API,
            path=svwb.list_path(lang, offset),
            parse=svwb.parse_list,
            bypass_resume=True,
            media_type="application/json",
        )
        listed = page.value
        if count is not None and listed.count != count:
            msg = f"{lang}: list total changed from {count} to {listed.count} mid-run"
            raise FetchError(msg)
        count = listed.count
        if not listed.card_ids or seen.intersection(listed.card_ids):
            msg = f"{lang}: page at offset {offset} is empty or repeats a card"
            raise FetchError(msg)
        seen.update(listed.card_ids)
        offset += svwb.PAGE_SIZE
    if len(seen) != count:
        msg = f"{lang}: pages list {len(seen)} cards, the API says {count}"
        raise FetchError(msg)
    return count
