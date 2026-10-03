"""Frozen card-page identity and image validation, without crawl entrypoints."""

from dataclasses import dataclass
from typing import TYPE_CHECKING
from urllib.parse import urljoin

from sve_carddb.template_semantics.v1.html import (
    attribute,
    parse,
    require_one,
    select_all,
    select_one,
)
from sve_carddb.template_semantics.v1.urls import canonicalize
from sve_carddb.template_semantics.v1.validation import ValidationError, decode_html

if TYPE_CHECKING:
    from selectolax.lexbor import LexborNode
MIN_PAGE_BYTES = 1000
CARD_DIR = "https://shadowverse-evolve.com/cardlist/"


@dataclass(frozen=True, slots=True)
class CardPage:
    """What the crawler needs from a card page: its images, in page order."""

    card_number: str
    name: str
    image_urls: list[str]
    image_originals: list[str]


def image_url(src: str, page_dir: str = CARD_DIR) -> str:
    """Resolve an `<img src>` from a card page in `page_dir`."""
    return canonicalize(urljoin(page_dir, src))


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
