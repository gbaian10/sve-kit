"""The Japanese official site: URLs, discovery and page checks.

Pure functions: no network, no files. Selectors were checked against pages
fetched during research; see tests/fixtures/official_jp.
"""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING
from urllib.parse import quote, unquote, urljoin, urlsplit

from sve_carddb.fetch.validate import ValidationError, decode_html
from sve_carddb.html import (
    Queryable,
    attribute,
    parse,
    require_one,
    select_all,
    select_one,
)
from sve_carddb.manifest import Region
from sve_carddb.store import relpath
from sve_carddb.urls import canonicalize

if TYPE_CHECKING:
    from pathlib import PurePosixPath

    from selectolax.lexbor import LexborNode

HOST = "shadowverse-evolve.com"
BASE = f"https://{HOST}"
CARD_DIR = f"{BASE}/cardlist/"
REGION = Region.JP
PAGE_SIZE = 15
MIN_PAGE_BYTES = 1000

_MAX_PAGE = re.compile(r"var\s+max_page\s*=\s*(\d+)\s*;")
_SET_CODE = re.compile(r"[A-Z0-9][A-Za-z0-9-]*")


@dataclass(frozen=True, slots=True)
class CardSet:
    """A product as listed in the card search form."""

    code: str
    name: str


@dataclass(frozen=True, slots=True)
class ListPage:
    """Card numbers on one list page; `max_page`/`total` only on page 1."""

    card_numbers: list[str]
    max_page: int | None = None
    total: int | None = None


@dataclass(frozen=True, slots=True)
class CardPage:
    """What the crawler needs from a card page: its images, in page order."""

    card_number: str
    name: str
    image_urls: list[str]
    image_originals: list[str]


def allowed(url: str) -> bool:
    """Only this site's pages and images may be requested, including after redirects."""
    parts = urlsplit(url)
    return parts.scheme == "https" and parts.hostname == HOST


# --- URLs -----------------------------------------------------------------


def sets_url() -> str:
    """The card search page, whose form lists every product."""
    return canonicalize(f"{BASE}/cardlist/")


def list_url(set_code: str, page: int) -> str:
    """List page `page` of a product, in the text view.

    Page 1 is the full page (it carries the page count and total); later
    pages are the fragments the site loads while scrolling.
    """
    code = quote(set_code, safe="")
    if page == 1:
        return canonicalize(
            f"{BASE}/cardlist/cardsearch/?expansion_name={code}&view=text"
        )
    return canonicalize(
        f"{BASE}/cardlist/cardsearch_ex?expansion_name={code}&view=text&page={page}"
    )


def card_url(card_number: str) -> str:
    """The card page, rebuilt from the number: list links carry extra query parameters."""
    return canonicalize(f"{CARD_DIR}?cardno={quote(card_number, safe='')}")


def image_url(src: str, page_dir: str = CARD_DIR) -> str:
    """Resolve an `<img src>` from a card page in `page_dir`."""
    return canonicalize(urljoin(page_dir, src))


# --- local paths ------------------------------------------------------------


def sets_path() -> PurePosixPath:
    """Where the card search page is stored."""
    return relpath("raw", REGION.value, "sets.html.zst")


def list_path(set_code: str, page: int) -> PurePosixPath:
    """Where a list page is stored."""
    return relpath("raw", REGION.value, "list", set_code, f"{page}.html.zst")


def card_path(card_number: str) -> PurePosixPath:
    """Where a card page is stored; the number is kept as-is, e.g. with `Ⓢ`."""
    return relpath("raw", REGION.value, "card", f"{card_number}.html.zst")


def image_path(url: str, region: Region = REGION) -> PurePosixPath:
    """Where an image is stored: the official path below `cardlist/`, unchanged."""
    marker = "/wp-content/images/cardlist/"
    path = urlsplit(url).path
    if marker not in path:
        msg = f"not a card image URL: {url}"
        raise ValidationError(msg)
    segments = [unquote(s) for s in path.split(marker, 1)[1].split("/")]
    return relpath("media", "images", region.value, *segments)


# --- page checks and discovery ----------------------------------------------


def parse_sets(body: bytes) -> list[CardSet]:
    """Products in the search form, without the "指定なし" placeholder."""
    tree = parse(decode_html(body, min_bytes=MIN_PAGE_BYTES))
    select = require_one(tree, "select[name=expansion_name]")
    sets: list[CardSet] = []
    for option in select_all(select, "option"):
        code = attribute(option, "value") or ""
        if not code:
            continue
        if not _SET_CODE.fullmatch(code):
            msg = f"unexpected product code {code!r}"
            raise ValidationError(msg)
        sets.append(CardSet(code=code, name=option.text(strip=True)))
    if not sets:
        msg = "the search form lists no products"
        raise ValidationError(msg)
    return sets


def parse_list_first(body: bytes) -> ListPage:
    """Page 1 of a product list: card numbers, page count and total."""
    html = decode_html(body, min_bytes=MIN_PAGE_BYTES)
    tree = parse(html)
    require_one(tree, "ul.cardlist-Result_List")
    match = _MAX_PAGE.search(html)
    if match is None:
        msg = "list page has no max_page"
        raise ValidationError(msg)
    total = _int_text(require_one(tree, "span.num.bold"), "total")
    max_page = int(match.group(1))
    numbers = _card_numbers(tree)
    if total < 1 or max_page < 1:
        msg = f"list reports {total} cards on {max_page} pages"
        raise ValidationError(msg)
    if max_page != -(-total // PAGE_SIZE):
        msg = f"{total} cards do not fit {max_page} pages of {PAGE_SIZE}"
        raise ValidationError(msg)
    _check_page_size(numbers, page=1, max_page=max_page, total=total)
    return ListPage(card_numbers=numbers, max_page=max_page, total=total)


def parse_list_more(body: bytes, *, page: int, max_page: int, total: int) -> ListPage:
    """A later list page (the scrolling fragment)."""
    tree = parse(decode_html(body, min_bytes=1))
    numbers = _card_numbers(tree)
    _check_page_size(numbers, page=page, max_page=max_page, total=total)
    return ListPage(card_numbers=numbers)


def parse_card(
    body: bytes, *, expected_number: str, page_dir: str = CARD_DIR
) -> CardPage:
    """A card page: it must be the card we asked for, and have at least one image.

    The English site shares this layout; `page_dir` resolves its image paths.
    """
    tree = parse(decode_html(body, min_bytes=MIN_PAGE_BYTES))
    detail = require_one(tree, ".cardlist-Detail")
    number = _card_page_number(detail)
    if number != expected_number:
        msg = f"asked for {expected_number}, page shows {number}"
        raise ValidationError(msg)
    name = require_one(detail, ".ttl").text(strip=True)
    if not name:
        msg = f"{number} has no card name"
        raise ValidationError(msg)
    originals = [
        src for img in select_all(detail, ".img img") if (src := attribute(img, "src"))
    ]
    if not originals:
        msg = f"{number} has no card image"
        raise ValidationError(msg)
    return CardPage(
        card_number=number,
        name=name,
        image_urls=[image_url(src, page_dir) for src in originals],
        image_originals=originals,
    )


def _card_page_number(detail: LexborNode) -> str:
    # With an illustrator the number is in `.name`; without one it is in `.heading`.
    illustrator = require_one(detail, ".illustrator")
    node = select_one(illustrator, ".name") or require_one(illustrator, ".heading")
    return node.text(strip=True)


def _card_numbers(tree: Queryable) -> list[str]:
    numbers = [n.text(strip=True) for n in select_all(tree, "li .number")]
    if len(set(numbers)) != len(numbers):
        msg = "duplicate card numbers on one page"
        raise ValidationError(msg)
    return numbers


def _check_page_size(
    numbers: list[str], *, page: int, max_page: int, total: int
) -> None:
    if not 1 <= page <= max_page:
        msg = f"page {page} is outside 1..{max_page}"
        raise ValidationError(msg)
    expected = PAGE_SIZE if page < max_page else total - PAGE_SIZE * (max_page - 1)
    if len(numbers) != expected:
        msg = f"page {page} has {len(numbers)} cards, expected {expected}"
        raise ValidationError(msg)


def _int_text(node: LexborNode, what: str) -> int:
    text = node.text(strip=True).replace(",", "")
    if not text.isdigit():
        msg = f"{what} is not a number: {text!r}"
        raise ValidationError(msg)
    return int(text)
