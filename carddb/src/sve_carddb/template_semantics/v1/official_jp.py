"""Transcribe Japanese official card pages into structured records.

The output keeps the page's own values (for example `-` for "no cost") so that
nothing is lost before the database schema is decided. Icons inside ability
text become `{alt}` tokens and line breaks become newlines.
"""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.template_semantics.v1.html import (
    Queryable,
    attribute,
    parse,
    require_one,
    select_all,
    select_one,
)
from sve_carddb.template_semantics.v1.page import MIN_PAGE_BYTES
from sve_carddb.template_semantics.v1.validation import ValidationError, decode_html

if TYPE_CHECKING:
    from selectolax.lexbor import LexborNode

# A line made only of these marks splits the text into sections. What follows it is
# not always token details: some spells print their main effect there.
_SECTION_SEPARATOR = re.compile(r"^[―─ー-]{5,}$")
_TRAIT_PART = re.compile(r"ジオ・テオゴニア|〈[^〉]+〉|[^・]+")
_QA_TITLE = re.compile(r"^(?P<id>Q\d+)\s*[（(](?P<date>[^）)]+)[）)]$")
_STAT_HEADINGS = {
    "status-Item-Cost": "cost",
    "status-Item-Power": "power",
    "status-Item-Hp": "hp",
}


@dataclass(frozen=True, slots=True)
class Face:
    """One side of a card; double-faced cards have two."""

    name: str
    card_class: str
    card_type: str
    traits: list[str]
    rarity: str
    product: str | None
    title: str | None
    cost: str
    power: str
    hp: str
    text: str | None
    sections: list[str]
    flavor: str | None
    illustrator: str | None
    image: str
    trait_raw: str


@dataclass(frozen=True, slots=True)
class QA:
    """A ruling printed on the card page."""

    id: str
    date: str
    question: str
    answer: str


@dataclass(frozen=True, slots=True)
class ProductHint:
    """Product evidence printed below the card, before product identity is resolved."""

    name: str
    date: str | None
    links: list[str]


@dataclass(frozen=True, slots=True)
class RelatedHint:
    """An official link, without inferring the relationship or card identity."""

    label: str
    href: str


@dataclass(frozen=True, slots=True)
class CardRecord:
    """Everything the card page says about one card number."""

    number: str
    faces: list[Face]
    release_date: str | None
    errata_url: str | None
    notes: list[str]
    qa: list[QA]
    products: list[ProductHint]
    related_cards: list[RelatedHint]


def extract_card(body: bytes, *, number: str) -> CardRecord:
    """Transcribe a stored card page."""
    tree = parse(decode_html(body, min_bytes=MIN_PAGE_BYTES))
    detail = require_one(tree, ".cardlist-Detail")
    faces = [
        _face(inner, number=number)
        for inner in select_all(detail, ".cardlist-Detail_Box_Inner")
    ]
    if not faces:
        msg = f"{number} has no card face"
        raise ValidationError(msg)
    errata = select_one(detail, ".illustrator a[href*='/errata/']")
    under = select_one(tree, ".cardlist-Under")
    return CardRecord(
        number=number,
        faces=faces,
        release_date=_release_date(under),
        errata_url=attribute(errata, "href") if errata is not None else None,
        notes=_notes(detail),
        qa=_qa(under),
        products=_products(under),
        related_cards=_related_cards(tree),
    )


def _face(inner: LexborNode, *, number: str) -> Face:
    info: dict[str, str] = {}
    for row in select_all(inner, ".info dl"):
        key = _text(require_one(row, "dt"))
        if key in info:
            msg = f"{number} has duplicate card info {key!r}"
            raise ValidationError(msg)
        info[key] = _text(require_one(row, "dd"))
    stats = _stats(inner)
    detail = select_one(inner, ".detail")
    text, *sections = _split_sections(_render(detail) if detail is not None else None)
    flavor = select_one(inner, ".speech")
    image = require_one(inner, ".img img")
    image_src = attribute(image, "src")
    if not image_src:
        msg = f"{number} has a card image without src"
        raise ValidationError(msg)
    name = _text(require_one(inner, ".ttl"))
    if not name:
        msg = f"{number} has an empty card name"
        raise ValidationError(msg)
    trait_raw = _required(info, "タイプ")
    return Face(
        name=name,
        card_class=_required(info, "クラス"),
        card_type=_required(info, "カード種類"),
        traits=_traits(trait_raw),
        rarity=_required(info, "レアリティ"),
        product=info.get("収録商品"),
        title=info.get("タイトル"),
        cost=stats["cost"],
        power=stats["power"],
        hp=stats["hp"],
        text=text,
        sections=[section for section in sections if section is not None],
        flavor=_render(flavor) if flavor is not None else None,
        illustrator=_illustrator(inner, number=number),
        image=image_src,
        trait_raw=trait_raw,
    )


def _required(info: dict[str, str], key: str) -> str:
    try:
        value = info[key]
    except KeyError:
        msg = f"card info has no {key!r}"
        raise ValidationError(msg) from None
    if not value:
        msg = f"card info has empty {key!r}"
        raise ValidationError(msg)
    return value


def _stats(inner: LexborNode) -> dict[str, str]:
    stats: dict[str, str] = {}
    for item in select_all(inner, ".status-Item"):
        classes = (attribute(item, "class") or "").split()
        key = next((_STAT_HEADINGS[c] for c in classes if c in _STAT_HEADINGS), None)
        heading = select_one(item, ".heading")
        if key is None or heading is None:
            continue
        value = _text(item).removeprefix(_text(heading)).strip()
        if not value:
            msg = f"card status has empty {key}"
            raise ValidationError(msg)
        stats[key] = value
    missing = {"cost", "power", "hp"} - stats.keys()
    if missing:
        msg = f"card status has no {sorted(missing)}"
        raise ValidationError(msg)
    return stats


def _traits(value: str) -> list[str]:
    if value == "-":
        return []
    parts = _TRAIT_PART.findall(value)
    if "・".join(parts) != value:
        msg = f"malformed trait list {value!r}"
        raise ValidationError(msg)
    return parts


def _illustrator(inner: LexborNode, *, number: str) -> str | None:
    # The errata notice reuses the `.illustrator` class; only the real one has `.heading`.
    # Cards without an illustrator credit show the card number in `.heading` instead.
    for node in select_all(inner, ".illustrator"):
        heading = select_one(node, ".heading")
        if heading is not None:
            name = _text(heading)
            return name if name != number else None
    return None


def _notes(detail: LexborNode) -> list[str]:
    # Errata and distribution notices share the `.illustrator` class but have no `.heading`.
    return [
        text
        for node in select_all(detail, ".illustrator")
        if select_one(node, ".heading") is None and (text := _render(node))
    ]


def _release_date(under: LexborNode | None) -> str | None:
    if under is None:
        return None
    date = select_one(under, ".cardlist-Detail_Products .date")
    return _text(date) if date is not None else None


def _products(under: LexborNode | None) -> list[ProductHint]:
    if under is None:
        return []
    products: list[ProductHint] = []
    for item in select_all(under, ".cardlist-Detail_Products_Inner"):
        name = _text(require_one(item, ".ttl"))
        if not name:
            msg = "product hint has no name"
            raise ValidationError(msg)
        date = select_one(item, ".date")
        products.append(
            ProductHint(
                name=name,
                date=_text(date) if date is not None else None,
                links=[
                    href
                    for link in select_all(item, "a")
                    if (href := attribute(link, "href")) is not None
                ],
            )
        )
    return products


def _related_cards(tree: Queryable) -> list[RelatedHint]:
    return [
        RelatedHint(label=_text(link), href=href)
        for link in select_all(tree, ".cardlist-Detail_Relation a[href*='cardno=']")
        if (href := attribute(link, "href")) is not None
    ]


def _qa(under: LexborNode | None) -> list[QA]:
    if under is None:
        return []
    items: list[QA] = []
    for item in select_all(under, ".cardlist-Detail_QA .qa-List_Item"):
        title = _text(require_one(item, ".qa-List_Ttl"))
        match = _QA_TITLE.match(title)
        if match is None:
            msg = f"unexpected Q&A title {title!r}"
            raise ValidationError(msg)
        items.append(
            QA(
                id=match["id"],
                date=match["date"],
                question=_qa_text(require_one(item, ".qa-List_Txt-Q")),
                answer=_qa_text(require_one(item, ".qa-List_Txt-A")),
            )
        )
    return items


def _qa_text(node: LexborNode) -> str:
    # Drop the decorative leading "Q" / "A" marker.
    marker = select_one(node, ".Garamond")
    text = _render(node) or ""
    if marker is not None:
        text = text.removeprefix(_text(marker))
    return text.strip()


def _split_sections(text: str | None) -> list[str | None]:
    """The text before the first separator line, then every later section."""
    if text is None:
        return [None]
    sections: list[list[str]] = [[]]
    for line in text.split("\n"):
        if _SECTION_SEPARATOR.match(line):
            sections.append([])
        else:
            sections[-1].append(line)
    return ["\n".join(lines).strip() for lines in sections]


def _render(node: LexborNode) -> str:
    """Text with `<br>` as newlines and icons as `{alt}`."""
    parts: list[str] = []
    _walk(node, parts)
    lines = [
        re.sub(r"[ \t\r\f\v]+", " ", line).strip()
        for line in "".join(parts).split("\n")
    ]
    return "\n".join(lines).strip()


def _walk(node: LexborNode, parts: list[str]) -> None:
    for child in node.iter(include_text=True):
        tag = child.tag
        if tag == "-text":
            parts.append((child.text_content or "").replace("\n", " "))
        elif tag == "br":
            parts.append("\n")
        elif tag == "img":
            parts.append("{" + (attribute(child, "alt") or "?") + "}")
        elif tag in {"p", "div"}:
            if parts and not "".join(parts).endswith("\n"):
                parts.append("\n")
            _walk(child, parts)
        else:
            _walk(child, parts)


def _text(node: LexborNode) -> str:
    return node.text(strip=True)
