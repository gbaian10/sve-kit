"""Transcribe EN sources using the independently measured legacy icon notation."""

import re
from dataclasses import dataclass
from pathlib import PurePosixPath
from typing import TYPE_CHECKING
from urllib.parse import urlsplit

from sve_carddb.ingest.http.validate import ValidationError, decode_html
from sve_carddb.parse.html import (
    Queryable,
    attribute,
    parse,
    require_one,
    select_all,
    select_one,
)
from sve_carddb.parse.pages import official_en as en
from sve_carddb.parse.pages.extract_jp import ProductHint, RelatedHint
from sve_carddb.parse.pages.official_jp import MIN_PAGE_BYTES

if TYPE_CHECKING:
    from selectolax.lexbor import LexborNode

_STAT_HEADINGS = {
    "status-Item-Cost": "cost",
    "status-Item-Power": "power",
    "status-Item-Hp": "hp",
}
_SEPARATOR = re.compile(r"^[―─ー-]{5,}$")
_WHITESPACE = re.compile(r"[ \t\r\f\v]+")


@dataclass(frozen=True, slots=True)
class Face:
    name: str
    info: dict[str, str]
    stats: dict[str, str]
    text: str | None
    sections: list[str]
    raw_text: str | None
    speech: str | None
    illustrator: str | None
    image: str
    trait_raw: str
    traits: list[str]


@dataclass(frozen=True, slots=True)
class QA:
    title: str
    question: str
    answer: str


@dataclass(frozen=True, slots=True)
class CardRecord:
    number: str
    faces: list[Face]
    release_date: str | None
    errata_url: str | None
    notes: list[str]
    qa: list[QA]
    products: list[ProductHint]
    related_cards: list[RelatedHint]


def extract_card(body: bytes, *, number: str) -> CardRecord:
    """Validate exact page identity before transcribing all faces and page metadata."""
    en.parse_card(body, expected_number=number)
    tree = parse(decode_html(body, min_bytes=MIN_PAGE_BYTES))
    detail = require_one(tree, ".cardlist-Detail")
    faces = [_face(inner) for inner in select_all(detail, ".cardlist-Detail_Box_Inner")]
    if not 1 <= len(faces) <= 2:  # ruff: ignore[magic-value-comparison] -- the registry permits one or two physical faces
        raise ValidationError("EN card must have one or two faces")
    errata = select_one(detail, ".illustrator a[href*='/errata/']")
    date = select_one(tree, ".cardlist-Detail_Products .date")
    return CardRecord(
        number=number,
        faces=faces,
        release_date=render(date) if date is not None else None,
        errata_url=attribute(errata, "href") if errata is not None else None,
        notes=[
            render(node)
            for node in select_all(detail, ".illustrator")
            if select_one(node, ".heading") is None
            and select_one(node, ".name") is None
        ],
        qa=_qa(tree),
        products=_product_hints(tree),
        related_cards=[
            RelatedHint(label=render(link), href=href)
            for link in select_all(tree, ".cardlist-Detail_Relation a[href*='cardno=']")
            if (href := attribute(link, "href")) is not None
        ],
    )


def _face(inner: LexborNode) -> Face:
    info: dict[str, str] = {}
    for row in select_all(inner, ".info dl"):
        key = require_one(row, "dt").text(strip=True)
        if not key or key in info:
            raise ValidationError("Missing or duplicate EN info key")
        info[key] = render(require_one(row, "dd"))
    for key in ("Class", "Card Type", "Rarity"):
        if not info.get(key):
            raise ValidationError("Missing required EN info value")
    if "Trait" not in info:
        raise ValidationError("Missing required EN trait label")
    name = require_one(inner, ".ttl").text(strip=True)
    if not name:
        raise ValidationError("Empty EN card name")
    image = attribute(require_one(inner, ".img img"), "src")
    if not image:
        raise ValidationError("EN card image has no src")
    detail = select_one(inner, ".detail")
    raw_text = render(detail) if detail is not None else None
    text, sections = _sections(raw_text)
    speech = select_one(inner, ".speech")
    trait_raw = info["Trait"]
    return Face(
        name=name,
        info=info,
        stats=_stats(inner),
        text=text,
        sections=sections,
        raw_text=raw_text,
        speech=render(speech) if speech is not None else None,
        illustrator=_credit(inner),
        image=image,
        trait_raw=trait_raw,
        traits=[] if trait_raw in {"-", ""} else trait_raw.split(" / "),
    )


def _stats(inner: LexborNode) -> dict[str, str]:
    result: dict[str, str] = {}
    for item in select_all(inner, ".status-Item"):
        classes = (attribute(item, "class") or "").split()
        keys = [value for key, value in _STAT_HEADINGS.items() if key in classes]
        if not keys:
            continue
        if len(keys) != 1 or keys[0] in result:
            raise ValidationError("Duplicate or ambiguous EN stat")
        heading = require_one(item, ".heading").text(strip=True)
        if not heading:
            raise ValidationError("Empty EN stat heading")
        value = item.text(strip=True).removeprefix(heading).strip()
        if not value:
            raise ValidationError("Empty EN stat value")
        result[keys[0]] = value
    if set(result) != {"cost", "power", "hp"}:
        raise ValidationError("EN card has incomplete stats")
    return result


def _credit(inner: LexborNode) -> str | None:
    for item in select_all(inner, ".illustrator"):
        if select_one(item, ".name") is not None:
            heading = select_one(item, ".heading")
            return render(heading) if heading is not None else None
    return None


def _sections(text: str | None) -> tuple[str | None, list[str]]:
    if text is None:
        return None, []
    parts: list[list[str]] = [[]]
    for line in text.split("\n"):
        if _SEPARATOR.fullmatch(line):
            parts.append([])
        else:
            parts[-1].append(line)
    rendered = ["\n".join(part).strip() for part in parts]
    return rendered[0], rendered[1:]


def _product_hints(tree: Queryable) -> list[ProductHint]:
    return [
        ProductHint(
            name=render(require_one(item, ".ttl")),
            date=render(date) if (date := select_one(item, ".date")) else None,
            links=[
                href
                for link in select_all(item, "a")
                if (href := attribute(link, "href")) is not None
            ],
        )
        for item in select_all(tree, ".cardlist-Detail_Products_Inner")
    ]


def _qa(tree: Queryable) -> list[QA]:
    return [
        QA(
            title=render(require_one(item, ".qa-List_Ttl")),
            question=qa_text(require_one(item, ".qa-List_Txt-Q")),
            answer=qa_text(require_one(item, ".qa-List_Txt-A")),
        )
        for item in select_all(tree, ".cardlist-Detail_QA .qa-List_Item")
    ]


def qa_text(node: LexborNode) -> str:
    """Render Q&A text without the decorative leading Q/A marker."""
    marker = select_one(node, ".Garamond")
    text = render(node)
    return text.removeprefix(render(marker)).strip() if marker is not None else text


def render(node: LexborNode) -> str:
    """Keep breaks and the measured `{filename_stem|alt}` EN icon notation."""
    parts: list[str] = []
    _walk(node, parts)
    return "\n".join(
        _WHITESPACE.sub(" ", line).strip() for line in "".join(parts).split("\n")
    ).strip()


def _walk(node: LexborNode, parts: list[str]) -> None:
    for child in node.iter(include_text=True):
        if child.tag == "-text":
            parts.append((child.text_content or "").replace("\n", " "))
        elif child.tag == "br":
            parts.append("\n")
        elif child.tag == "img":
            src, alt = attribute(child, "src"), attribute(child, "alt")
            if not src or not alt:
                raise ValidationError("EN text icon lacks src or alt")
            stem = PurePosixPath(urlsplit(src).path).stem
            if not stem:
                raise ValidationError("EN text icon lacks filename stem")
            parts.append("{" + stem + "|" + alt + "}")
        else:
            if (
                child.tag in {"p", "div"}
                and parts
                and not "".join(parts).endswith("\n")
            ):
                parts.append("\n")
            _walk(child, parts)
