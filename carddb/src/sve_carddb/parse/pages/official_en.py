"""The English official site: URLs, local paths and page checks.

Pure functions: no network, no files. The pages share the Japanese site's
markup (search form, `max_page`, `.number`, `.cardlist-Detail`), so parsing
is delegated to `official_jp`; see tests/fixtures/official_en.
"""

from typing import TYPE_CHECKING
from urllib.parse import quote, urlsplit

from sve_carddb.core.paths import relpath
from sve_carddb.core.regions import SourceRegion as Region
from sve_carddb.ingest.urls import canonicalize
from sve_carddb.parse.pages import official_jp as jp

if TYPE_CHECKING:
    from pathlib import PurePosixPath

HOST = "en.shadowverse-evolve.com"
BASE = f"https://{HOST}"
CARD_DIR = f"{BASE}/cards/"
REGION = Region.EN


def allowed(url: str) -> bool:
    """Only this site's pages and images may be requested, including after redirects."""
    parts = urlsplit(url)
    return parts.scheme == "https" and parts.hostname == HOST


# --- URLs -----------------------------------------------------------------


def sets_url() -> str:
    """The card search page, whose form lists every product."""
    return canonicalize(CARD_DIR)


def list_url(set_code: str, page: int) -> str:
    """List page `page` of a product in the text view; later pages are fragments."""
    code = quote(set_code, safe="")
    if page == 1:
        return canonicalize(f"{CARD_DIR}searchresults/?expansion={code}&view=text")
    return canonicalize(
        f"{CARD_DIR}searchresults_ex?expansion={code}&view=text&page={page}"
    )


def card_url(card_number: str) -> str:
    """The card page, rebuilt from the number: list links carry extra query parameters."""
    return canonicalize(f"{CARD_DIR}?cardno={quote(card_number, safe='')}")


# --- local paths ------------------------------------------------------------


def sets_path() -> PurePosixPath:
    """Where the card search page is stored."""
    return relpath("raw", REGION.value, "sets.html.zst")


def list_path(set_code: str, page: int) -> PurePosixPath:
    """Where a list page is stored."""
    return relpath("raw", REGION.value, "list", set_code, f"{page}.html.zst")


def card_path(card_number: str) -> PurePosixPath:
    """Where a card page is stored; the number is kept as-is, `EN` suffix included."""
    return relpath("raw", REGION.value, "card", f"{card_number}.html.zst")


def image_path(url: str) -> PurePosixPath:
    """Where an image is stored: the official path below `cardlist/`, unchanged."""
    return jp.image_path(url, REGION)


# --- page checks --------------------------------------------------------------


def parse_card(body: bytes, *, expected_number: str) -> jp.CardPage:
    """A card page: it must be the card we asked for, and have at least one image."""
    return jp.parse_card(body, expected_number=expected_number, page_dir=CARD_DIR)
