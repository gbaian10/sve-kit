"""Conservative frozen announcement staging, with no current or approval effects.

Consumers must verify boundaries other than ``nbsp_separator`` and any unused
lines, or report them as unresolved. A heading or document end can leave prose
inside a fragment; matched associations alone do not prove complete coverage.
"""

import re
from dataclasses import dataclass, field, replace
from datetime import date
from typing import TYPE_CHECKING
from urllib.parse import urljoin, urlsplit

from sve_carddb.card_extras.dates import parse_en_date
from sve_carddb.card_extras.errata_markup import NoticeMarkup, Piece, lines
from sve_carddb.core.json import digest
from sve_carddb.parse.html import attribute, parse, select_all

if TYPE_CHECKING:
    from selectolax.lexbor import LexborNode

    from sve_carddb.core.provenance import Source
    from sve_carddb.registry.records import Region

PARSER = "official-errata-staging-v1"
_WRAPPER = (
    "div.st-Container > div.st-Container_Inner > div.sw-Lower > div.sw-Lower_Wrapper"
)
_NUMBER = re.compile(r"[A-Z][A-Za-z0-9]*-[A-Za-z0-9Ⓢ]+")
_PATH = re.compile(r"/errata/[^/]+/?")
_PAIR = {
    "jp": {"(誤)": "before", "（誤）": "before", "(正)": "after", "（正）": "after"},
    "en": {"(Incorrect)": "before", "(Correct)": "after"},
}
_FIELD_LABELS = {
    "Card Name": "name",
    "カード名": "name",
    "タイプ": "traits",
    "カード種類": "card_type",
}
_FIELD_PREFIX = re.compile(
    r"^(" + "|".join(map(re.escape, _FIELD_LABELS)) + r")[：:][ \t]*"
)


@dataclass(frozen=True)
class RawSlice:
    start: int
    end: int
    sha256: str


@dataclass(frozen=True)
class Fragment:
    raw: RawSlice
    pieces: tuple[Piece, ...] = field(repr=False)
    field_label: str | None = None


@dataclass(frozen=True)
class ChangeBlock:
    ordinal: int
    before: Fragment
    after: Fragment
    context: tuple[str, ...] = field(repr=False)
    end_basis: str


@dataclass(frozen=True)
class ListedCards:
    numbers: tuple[str, ...]
    names: str = field(repr=False)


@dataclass(frozen=True)
class StagedNotice:
    """Retain diagnostics without claiming that all announcement content was used.

    ``unused_trailing_lines`` counts retained outside context since the last
    heading or block reset, including the last heading itself. Earlier context
    discarded by a heading is not counted, even when there are no blocks.
    """

    source: Source
    region: Region
    listed: ListedCards
    blocks: tuple[ChangeBlock, ...]
    heading_date_raw: str | None
    heading_date: str | None
    issues: tuple[str, ...]
    images: tuple[tuple[str, str], ...] = field(repr=False)
    unused_trailing_lines: int


@dataclass(frozen=True)
class BlockAssociation:
    card_no: str
    block_ordinal: int
    basis: str


@dataclass(frozen=True)
class Associations:
    matched: tuple[BlockAssociation, ...]
    unassigned_cards: tuple[str, ...]
    unassigned_blocks: tuple[int, ...]


def associate_blocks(notice: StagedNotice, names: dict[str, str]) -> Associations:
    """Use explicit context before inspecting any corrected card wording."""
    matched: list[BlockAssociation] = []
    for block in () if notice.issues else notice.blocks:
        last = block.context[-1].strip() if block.context else ""
        explicit = [number for number in notice.listed.numbers if last == number]
        named = [
            number
            for number in notice.listed.numbers
            if last and names.get(number) == last
        ]
        if explicit:
            targets, basis = explicit, "explicit_number"
        elif len(named) == 1:
            targets, basis = named, "explicit_name"
        elif named:
            continue
        elif len(notice.blocks) == 1 and last in (
            {"▼修正内容"} if notice.region == "jp" else {"Changes"}
        ):
            targets, basis = list(notice.listed.numbers), "single_shared_block"
        else:
            continue
        matched.extend(
            BlockAssociation(number, block.ordinal, basis) for number in targets
        )
    cards = {item.card_no for item in matched}
    blocks = {item.block_ordinal for item in matched}
    return Associations(
        tuple(matched),
        tuple(number for number in notice.listed.numbers if number not in cards),
        tuple(block.ordinal for block in notice.blocks if block.ordinal not in blocks),
    )


def _one(root: LexborNode, selector: str) -> LexborNode:
    nodes = select_all(root, selector)
    if len(nodes) != 1:
        raise ValueError("Errata layout requires exactly one " + selector)
    return nodes[0]


def _fragment(pieces: list[Piece], html: str) -> Fragment:
    retained = [
        piece for piece in pieces if piece.kind == "image" or piece.value.strip()
    ]
    if not retained:
        raise ValueError("Errata pair has an empty side")
    start, end = retained[0].start, retained[-1].end
    # Original bytes, not DOM serialization or comparison tokens, identify the evidence.
    raw = html[start:end].encode("utf-8")
    label = None
    if retained[0].kind == "text" and (match := _FIELD_PREFIX.match(retained[0].value)):
        label = _FIELD_LABELS[match[1]]
        pieces = [
            replace(piece, value=piece.value[match.end() :])
            if piece is retained[0]
            else piece
            for piece in pieces
        ]
    return Fragment(
        RawSlice(
            len(html[:start].encode("utf-8")),
            len(html[:end].encode("utf-8")),
            digest(raw),
        ),
        tuple(pieces),
        label,
    )


class _Pairs:
    def __init__(self, markup: NoticeMarkup, region: Region) -> None:
        self.markup = markup
        self.region = region
        self.result: list[ChangeBlock] = []
        self.issues: set[str] = set()
        self.context: list[str] = []
        self.before: list[Piece] = []
        self.after: list[Piece] = []
        self.phase = "outside"
        self.block_context: tuple[str, ...] = ()
        self.arrows = 0

    def finish(self, end_basis: str) -> None:
        had_pair = self.phase != "outside"
        if self.phase == "after":
            if self.arrows != 1:
                raise ValueError("Errata pair requires exactly one change arrow")
            before = _fragment(self.before, self.markup.html)
            after = _fragment(self.after, self.markup.html)
            if before.field_label != after.field_label:
                self.issues.add("inconsistent_field_label")
            self.result.append(
                ChangeBlock(
                    len(self.result), before, after, self.block_context, end_basis
                )
            )
        elif self.phase == "before":
            self.issues.add("missing_correct_marker")
        self.phase = "outside"
        self.before, self.after = [], []
        self.arrows = 0
        if had_pair:
            self.context = []

    def marker(self, marker: str) -> None:
        if marker == "before":
            if self.phase != "outside":
                self.issues.add("pair_truncated_by_incorrect_marker")
            self.finish("next_incorrect_marker")
            self.phase = "before"
            self.block_context = tuple(self.context)
        elif self.phase != "before":
            self.issues.add("orphan_correct_marker")
            self.finish("orphan_correct_marker")
        else:
            self.phase = "after"

    def accept(self, line: tuple[Piece, ...]) -> None:
        value = "".join(piece.value for piece in line).strip()
        has_image = any(piece.kind == "image" for piece in line)
        if line[0].kind == "heading" or value.startswith("▼"):
            self.finish("heading")
            self.context = [value] if value else []
        elif not value and not has_image:
            if (
                self.phase == "after"
                and _content(self.after)
                and any("\xa0" in piece.value for piece in line)
            ):
                self.finish("nbsp_separator")
        elif (marker := _PAIR[self.region].get(value)) is not None:
            self.marker(marker)
        elif value == "↓":
            if self.phase != "before":
                self.issues.add("orphan_change_arrow")
            else:
                self.arrows += 1
        elif self.phase in {"before", "after"}:
            target = self.before if self.phase == "before" else self.after
            if target:
                target.append(Piece("break", line[0].start, line[0].start, "\n"))
            target.extend(line)
        else:
            self.context.append(value)


def _content(pieces: list[Piece]) -> bool:
    return any(piece.kind == "image" or piece.value.strip() for piece in pieces)


def _blocks(
    markup: NoticeMarkup, region: Region
) -> tuple[tuple[ChangeBlock, ...], tuple[str, ...], int]:
    pairs = _Pairs(markup, region)
    for line in lines(markup.pieces):
        pairs.accept(line)
    pairs.finish("end_of_document")
    if not pairs.result:
        pairs.issues.add("no_paired_changes")
    return tuple(pairs.result), tuple(sorted(pairs.issues)), len(pairs.context)


def _cards(inner: LexborNode, region: Region) -> ListedCards:
    labels = (
        ("カード番号", "カード名")
        if region == "jp"
        else ("Card Number(s)", "Card Name")
    )
    values: dict[str, str] = {}
    for node in select_all(inner, ".sw-Info dl"):
        label = _one(node, "dt").text(strip=True)
        value = _one(node, "dd").text()
        if label in labels:
            if label in values:
                raise ValueError("Duplicate errata card-information label")
            values[label] = value
    if set(values) != set(labels):
        raise ValueError("Errata card-information labels are missing")
    numbers = tuple(_NUMBER.findall(values[labels[0]]))
    leftover = _NUMBER.sub("", values[labels[0]])
    if (
        not numbers
        or len(set(numbers)) != len(numbers)
        or leftover.strip(" \t\r\n,、/")
    ):
        raise ValueError("Unrecognized errata card-number list")
    return ListedCards(numbers, values[labels[1]])


def _heading_date(inner: LexborNode, region: Region) -> tuple[str | None, str | None]:
    times = select_all(inner, ".heading > .heading-top > time.time.Sans")
    if not times:
        return None, None
    if len(times) != 1:
        raise ValueError("Ambiguous errata heading date")
    raw = times[0].text()
    value = raw.strip()
    if region == "en":
        return raw, parse_en_date(value)
    match = re.fullmatch(r"([0-9]{4})\.([0-9]{2})\.([0-9]{2})", value)
    if match is None:
        return raw, None
    try:
        return raw, date(*map(int, match.groups())).isoformat()
    except ValueError:
        return raw, None


def parse_notice(raw: bytes, source: Source, *, region: Region) -> StagedNotice:
    """Transcribe explicit evidence; heading dates are not asserted publication dates."""
    if region not in {"jp", "en"}:
        raise ValueError("Errata parser requires an explicit JP or EN region")
    if source.sha256 != digest(raw) or source.parser_version != PARSER:
        raise ValueError("Errata raw hash/parser pin mismatch")
    host = "shadowverse-evolve.com" if region == "jp" else "en.shadowverse-evolve.com"
    parts = urlsplit(source.url)
    if (
        (parts.scheme, parts.netloc) != ("https", host)
        or not _PATH.fullmatch(parts.path)
        or parts.query
        or parts.fragment
        or source.kind != "official_page"
    ):
        raise ValueError("Errata source URL/region/media mismatch")
    html = raw.decode("utf-8")
    prefix = "eratta" if region == "jp" else "errata"
    selector = (
        _WRAPPER
        + f" > div.sw-Lower_Container > div.{prefix}-Detail > div.{prefix}-Detail_Inner"
    )
    roots = select_all(parse(html), selector)
    if len(roots) != 1:
        raise ValueError("Errata announcement container is missing or ambiguous")
    inner = roots[0]
    _one(inner, ".heading > h1.ttl")
    body = _one(inner, ".contents.sw-Txtarea")
    markup = NoticeMarkup(html)
    markup.feed(html)
    markup.close()
    if markup.bodies != 1:
        raise ValueError("Errata raw body container is ambiguous")
    blocks, issues, unused_trailing_lines = _blocks(markup, region)
    images = tuple(
        sorted(
            {
                (urljoin(source.url, src), attribute(node, "alt") or "")
                for node in select_all(body, "img")
                if (src := attribute(node, "src")) is not None
            }
        )
    )
    date_raw, parsed_date = _heading_date(inner, region)
    return StagedNotice(
        source,
        region,
        _cards(inner, region),
        blocks,
        date_raw,
        parsed_date,
        issues,
        images,
        unused_trailing_lines,
    )
