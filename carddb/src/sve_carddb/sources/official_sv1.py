"""shadowverse-portal.com, the official card list of the first digital game.

Pure functions: no network, no files. Kept locally to map SVE cards to digital
ones (by image similarity) and for the official Traditional Chinese text.

Card image URLs are built from a template, unlike the SVE sites where they
must be read from `<img src>`: every card page uses the same pattern, and
fetching ~5,900 pages just to read it back would triple the requests. When the
template misses, the card page's actual `<img src>` is used instead.
"""

from dataclasses import dataclass
from enum import StrEnum
from typing import TYPE_CHECKING
from urllib.parse import unquote, urljoin, urlsplit, urlunsplit

import orjson

from sve_carddb.fetch.validate import ValidationError, decode_html
from sve_carddb.html import attribute, parse, require_one, select_all, select_one
from sve_carddb.manifest import Region
from sve_carddb.store import relpath
from sve_carddb.urls import canonicalize

if TYPE_CHECKING:
    from pathlib import PurePosixPath

HOST = "shadowverse-portal.com"
BASE = f"https://{HOST}"
REGION = Region.SV1
LANGUAGES = ("ja", "en", "zh-tw")
CARD_PAGE_LANGUAGE = "ja"
FOLLOWER = 1
# About 5,900 cards in 2026; far fewer means a truncated or error response.
MIN_CARDS = 1000
MIN_PAGE_BYTES = 1000
# Portal card images run to about 0.7 MB; the SVE-site reserve would overstate need 4x.
IMAGE_RESERVE_BYTES = 3 * 1024 * 1024 // 2
_CARD_ID_DIGITS = 9
_IMAGE_PREFIX = "/image/card/"


class Face(StrEnum):
    """Which side of a card an image shows; the value is the official file prefix."""

    BASE = "C"
    EVOLVED = "E"


# Order of the images on a card page.
FACES = (Face.BASE, Face.EVOLVED)


@dataclass(frozen=True, slots=True)
class ApiCard:
    """The fields of one API card the crawler needs."""

    card_id: int
    name: str | None
    """Null for some tokens (385 of them in 2026)."""
    char_type: int

    @property
    def faces(self) -> list[Face]:
        """Images the card has: only followers have an evolved side."""
        if self.char_type == FOLLOWER:
            return [Face.BASE, Face.EVOLVED]
        return [Face.BASE]


@dataclass(frozen=True, slots=True)
class CardImages:
    """Images on a card page in page order, resolved and as written."""

    urls: list[str]
    originals: list[str]


def allowed(url: str) -> bool:
    """Only this site's API, pages and images may be requested."""
    parts = urlsplit(url)
    return parts.scheme == "https" and parts.hostname == HOST


# --- URLs -----------------------------------------------------------------


def api_url(lang: str) -> str:
    """Every card in one language."""
    return canonicalize(f"{BASE}/api/v1/cards?format=json&lang={lang}")


def card_url(card_id: int) -> str:
    """The card page, only fetched when the image template misses."""
    return canonicalize(f"{BASE}/card/{card_id}?lang={CARD_PAGE_LANGUAGE}")


def image_url(card_id: int, face: Face) -> str:
    """The image URL template seen on every card page."""
    return canonicalize(
        f"{BASE}/image/card/phase2/common/{face.value}/{face.value}_{card_id}.png"
    )


def resolve_image(src: str) -> str:
    """Resolve an `<img src>`; the query is a cache buster, so it is dropped."""
    parts = urlsplit(urljoin(f"{BASE}/card/", src))
    return canonicalize(urlunsplit((parts.scheme, parts.netloc, parts.path, "", "")))


# --- local paths ------------------------------------------------------------


def api_path(lang: str) -> PurePosixPath:
    """Where the API response is stored, as plain JSON."""
    return relpath("raw", REGION.value, "api", f"cards-{lang}.json")


def card_path(card_id: int) -> PurePosixPath:
    """Where a card page is stored."""
    return relpath("raw", REGION.value, "card", f"{card_id}.html.zst")


def image_path(url: str) -> PurePosixPath:
    """Where an image is stored: the official URL path, unchanged."""
    path = urlsplit(url).path
    if not path.startswith(_IMAGE_PREFIX):
        msg = f"not a card image URL: {url}"
        raise ValidationError(msg)
    segments = [unquote(s) for s in path.lstrip("/").split("/")]
    return relpath("media", "images", REGION.value, *segments)


# --- response checks ----------------------------------------------------------


def parse_cards(body: bytes) -> list[ApiCard]:
    """The API response: no errors, and enough well-formed, unique cards."""
    try:
        document: object = orjson.loads(body)
    except orjson.JSONDecodeError as exc:
        msg = "API response is not JSON"
        raise ValidationError(msg) from exc
    data = _member(document, "data")
    errors = _member(data, "errors")
    if errors != []:
        msg = f"API reports errors: {errors!r}"
        raise ValidationError(msg)
    raw_cards = _member(data, "cards")
    if not isinstance(raw_cards, list):
        msg = "API cards is not a list"
        raise ValidationError(msg)
    cards = [_card(item) for item in raw_cards]
    if len(cards) < MIN_CARDS:
        msg = f"API lists {len(cards)} cards, expected at least {MIN_CARDS}"
        raise ValidationError(msg)
    if len({card.card_id for card in cards}) != len(cards):
        msg = "API lists a card_id twice"
        raise ValidationError(msg)
    return cards


def parse_card_images(body: bytes) -> CardImages:
    """A card page: its title and at least one card image.

    Some tokens in the API have no card page (nor image); the site answers
    with its error page, which gives no images.
    """
    tree = parse(decode_html(body, min_bytes=MIN_PAGE_BYTES))
    if select_one(tree, "h1.el-heading-error") is not None:
        return CardImages(urls=[], originals=[])
    title = require_one(tree, "h1.card-main-title").text(strip=True)
    if not title:
        msg = "card page has no title"
        raise ValidationError(msg)
    originals = [
        src
        for img in select_all(tree, "div.card-main-image > img")
        if (src := attribute(img, "src"))
    ]
    if not originals:
        msg = f"card page {title!r} has no card image"
        raise ValidationError(msg)
    return CardImages(
        urls=[resolve_image(src) for src in originals], originals=originals
    )


def _member(value: object, key: str) -> object:
    if not isinstance(value, dict) or key not in value:
        msg = f"API response has no {key!r}"
        raise ValidationError(msg)
    member: object = value[key]
    return member


def _card(item: object) -> ApiCard:
    card_id = _member(item, "card_id")
    name = _member(item, "card_name")
    char_type = _member(item, "char_type")
    # bool is an int subclass; a flag here would mean the format changed.
    if (
        not isinstance(card_id, int)
        or isinstance(card_id, bool)
        or len(str(card_id)) != _CARD_ID_DIGITS
    ):
        msg = f"unexpected card_id {card_id!r}"
        raise ValidationError(msg)
    if not isinstance(name, str | None) or not isinstance(char_type, int):
        msg = f"card {card_id}: unexpected card_name or char_type"
        raise ValidationError(msg)
    return ApiCard(card_id=card_id, name=name, char_type=char_type)
