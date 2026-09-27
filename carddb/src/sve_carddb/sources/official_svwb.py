"""shadowverse-wb.com, the official card list of Shadowverse: Worlds Beyond.

Pure functions: no network, no files. Kept locally to map SVE cards to digital
ones (text and art, including alternate styles) and for the official Traditional
Chinese text. Images are addressed by the hashes the list API gives.

The card list API pages 30 cards at a time and picks the language from a `Lang`
header, not the URL. The manifest is keyed by URL, so each URL also carries a
`lang` query the server ignores; `headers` turns it back into the header.
"""

from dataclasses import dataclass
from typing import TYPE_CHECKING, TypeIs
from urllib.parse import parse_qs, urlsplit

import orjson

from sve_carddb.fetch.validate import ValidationError
from sve_carddb.manifest import Region
from sve_carddb.store import relpath
from sve_carddb.urls import canonicalize

if TYPE_CHECKING:
    from pathlib import PurePosixPath

HOST = "shadowverse-wb.com"
BASE = f"https://{HOST}"
REGION = Region.SVWB
# The site's own codes; `cht` is Traditional Chinese.
LANGUAGES = ("ja", "en", "cht")
PAGE_SIZE = 30
# About 900 cards with tokens in 2026; far fewer means an error response.
MIN_CARDS = 500
_LIST_PATH = "/web/CardList/cardList"
_OK = 1
# Japanese card art; the site also has per-language renders of the same art.
_IMAGE_PREFIX = "/uploads/card_image/jpn/card/"
_HASH_LENGTH = 32
_HEX = frozenset("0123456789abcdef")
# About 0.5 MB each in 2026.
IMAGE_RESERVE_BYTES = 1024 * 1024


@dataclass(frozen=True, slots=True)
class ListPage:
    """What the crawler checks on one page of the card list."""

    count: int
    """Cards in the whole list, repeated on every page."""
    card_ids: list[int]
    """This page's cards, in list order."""


def allowed(url: str) -> bool:
    """Only this site may be requested."""
    parts = urlsplit(url)
    return parts.scheme == "https" and parts.hostname == HOST


def list_url(lang: str, offset: int) -> str:
    """One page of the card list, tokens included."""
    if lang not in LANGUAGES:
        msg = f"unknown language {lang!r}"
        raise ValueError(msg)
    return canonicalize(
        f"{BASE}{_LIST_PATH}?include_token=1&lang={lang}&offset={offset}"
    )


def headers(url: str) -> tuple[tuple[str, str], ...]:
    """The `Lang` header named by a list URL's `lang` query."""
    parts = urlsplit(url)
    if parts.path != _LIST_PATH:
        return ()
    langs = parse_qs(parts.query).get("lang", [])
    if len(langs) != 1 or langs[0] not in LANGUAGES:
        msg = f"{url}: list URL without one known lang"
        raise ValueError(msg)
    return (("Lang", langs[0]),)


def list_path(lang: str, offset: int) -> PurePosixPath:
    """Where a list page is stored, as plain JSON."""
    return relpath("raw", REGION.value, "api", lang, f"cardList-{offset:04d}.json")


def image_url(image_hash: str) -> str:
    """A card image, as the card list page builds it from a hash."""
    return canonicalize(f"{BASE}{_IMAGE_PREFIX}{image_hash}.png")


def image_path(url: str) -> PurePosixPath:
    """Where an image is stored: the official URL path, unchanged."""
    path = urlsplit(url).path
    if not path.startswith(_IMAGE_PREFIX):
        msg = f"not a card image URL: {url}"
        raise ValidationError(msg)
    return relpath("media", "images", REGION.value, *path.lstrip("/").split("/"))


def image_hashes(body: bytes) -> list[str]:
    """Every image on a list page: base, evolved and alternate styles, in page order."""
    details = _member(orjson.loads(body), "data")
    cards = _member(details, "card_details")
    if not isinstance(cards, dict):
        msg = "API card_details is not an object"
        raise ValidationError(msg)
    found: list[str] = []
    for card in cards.values():
        common = _member(card, "common")
        found.append(_hash(_member(common, "card_image_hash")))
        evo = _member(card, "evo")
        # Cards without an evolved side have an empty list here.
        if isinstance(evo, dict) and evo:
            found.append(_hash(_member(evo, "card_image_hash")))
        styles = _member(card, "style_card_list")
        if not isinstance(styles, list):
            msg = "API style_card_list is not a list"
            raise ValidationError(msg)
        for style in styles:
            found.append(_hash(_member(style, "hash")))
            evo_hash = _member(style, "evo_hash")
            if evo_hash:
                found.append(_hash(evo_hash))
    return list(dict.fromkeys(found))


def _hash(value: object) -> str:
    if (
        not isinstance(value, str)
        or len(value) != _HASH_LENGTH
        or not _HEX.issuperset(value)
    ):
        msg = f"unexpected image hash {value!r}"
        raise ValidationError(msg)
    return value


def parse_list(body: bytes) -> ListPage:
    """A list page: success code, a plausible total and details for each card."""
    try:
        document: object = orjson.loads(body)
    except orjson.JSONDecodeError as exc:
        msg = "API response is not JSON"
        raise ValidationError(msg) from exc
    code = _member(_member(document, "data_headers"), "result_code")
    if code != _OK:
        msg = f"API result_code {code!r}"
        raise ValidationError(msg)
    data = _member(document, "data")
    count = _member(data, "count")
    if not _is_int(count) or count < MIN_CARDS:
        msg = f"API count {count!r}, expected at least {MIN_CARDS}"
        raise ValidationError(msg)
    ids = _member(data, "sort_card_id_list")
    if not isinstance(ids, list) or not all(_is_int(i) for i in ids):
        msg = "API sort_card_id_list is not a list of ints"
        raise ValidationError(msg)
    card_ids = [i for i in ids if _is_int(i)]
    if len(card_ids) > PAGE_SIZE or len(set(card_ids)) != len(card_ids):
        msg = f"API page lists {len(card_ids)} cards or repeats one"
        raise ValidationError(msg)
    details = _member(data, "card_details")
    if not isinstance(details, dict):
        msg = "API card_details is not an object"
        raise ValidationError(msg)
    missing = [i for i in card_ids if str(i) not in details]
    if missing:
        msg = f"API page has no details for {missing}"
        raise ValidationError(msg)
    return ListPage(count=count, card_ids=card_ids)


def _is_int(value: object) -> TypeIs[int]:
    # bool is an int subclass; a flag here would mean the format changed.
    return isinstance(value, int) and not isinstance(value, bool)


def _member(value: object, key: str) -> object:
    if not isinstance(value, dict) or key not in value:
        msg = f"API response has no {key!r}"
        raise ValidationError(msg)
    member: object = value[key]
    return member
