"""Conservative frozen announcement staging, with no current or approval effects.

A heading or document end can leave prose inside a fragment. The staging
report records boundary evidence for human review.
"""

import re
from dataclasses import dataclass, field
from datetime import date
from pathlib import PurePosixPath
from typing import TYPE_CHECKING
from urllib.parse import urlsplit

from sve_carddb.core.json import digest
from sve_carddb.domains.card_extras.dates import parse_en_date
from sve_carddb.domains.card_extras.errata_markup import NoticeMarkup, Piece, lines
from sve_carddb.domains.card_extras.reviewed_errata import Block, Slice
from sve_carddb.parse.html import parse, select_all

if TYPE_CHECKING:
    from selectolax.lexbor import LexborNode

    from sve_carddb.core.provenance import Source
    from sve_carddb.core.regions import Region

PARSER = "official-errata-staging-v2"
_WRAPPER = (
    "div.st-Container > div.st-Container_Inner > div.sw-Lower > div.sw-Lower_Wrapper"
)
_NUMBER = re.compile(r"[A-Z][A-Za-z0-9]*-[A-Za-z0-9Ⓢ]+")
_WHITESPACE = re.compile(r"[ \t\r\f\v]+")
_PATH = re.compile(r"/errata/[^/]+/?")
_PAIR = {
    "jp": {"(誤)": "before", "（誤）": "before", "(正)": "after", "（正）": "after"},
    "en": {"(Incorrect)": "before", "(Correct)": "after"},
}


@dataclass(frozen=True)
class ChangeBlock:
    evidence: Block = field(repr=False)
    end_basis: str


@dataclass(frozen=True)
class ListedCards:
    numbers: tuple[str, ...]
    name: str = field(repr=False)


@dataclass(frozen=True)
class StagedNotice:
    region: Region
    listed: ListedCards
    blocks: tuple[ChangeBlock, ...]
    heading_date_raw: str | None
    heading_date: str | None
    issues: tuple[str, ...]
    outside_text: tuple[str, ...] = field(repr=False)


def _one(root: LexborNode, selector: str) -> LexborNode:
    nodes = select_all(root, selector)
    if len(nodes) != 1:
        raise ValueError("Errata layout requires exactly one " + selector)
    return nodes[0]


def _icon(piece: Piece, region: Region) -> str:
    attrs = dict(piece.attrs)
    alt = attrs.get("alt")
    if region == "jp":
        return "{" + (alt or "?") + "}"
    stem = PurePosixPath(urlsplit(attrs.get("src") or "").path).stem
    if not stem or not alt:
        raise ValueError("EN errata text icon lacks filename stem or alt")
    return "{" + stem + "|" + alt + "}"


def _fragment(pieces: list[Piece], html: str, region: Region) -> tuple[str, Slice]:
    retained = [
        piece for piece in pieces if piece.kind == "image" or piece.value.strip()
    ]
    if not retained:
        raise ValueError("Errata pair has an empty side")
    start, end = retained[0].start, retained[-1].end
    text = "".join(
        _icon(piece, region)
        if piece.kind == "image"
        else piece.value.replace("\n", " ")
        if piece.kind == "text"
        else piece.value
        for piece in pieces
    )
    text = "\n".join(
        _WHITESPACE.sub(" ", line).strip() for line in text.split("\n")
    ).strip()
    # Transcribe for review, but hash the original UTF-8 rather than rendered text.
    return text, Slice(
        start=len(html[:start].encode("utf-8")),
        end=len(html[:end].encode("utf-8")),
        sha256=digest(html[start:end].encode("utf-8")),
    )


class _Pairs:
    def __init__(self, markup: NoticeMarkup, region: Region) -> None:
        self.markup = markup
        self.region = region
        self.result: list[ChangeBlock] = []
        self.issues: set[str] = set()
        self.context: list[str] = []
        self.outside_text: list[str] = []
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
            before, raw_before = _fragment(self.before, self.markup.html, self.region)
            after, raw_after = _fragment(self.after, self.markup.html, self.region)
            self.result.append(
                ChangeBlock(
                    Block(
                        ordinal=len(self.result),
                        context=self.block_context,
                        before=before,
                        after=after,
                        raw_before=raw_before,
                        raw_after=raw_after,
                    ),
                    end_basis,
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
            self.outside_text.extend(self.context)
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
        elif value:
            self.context.append(value)
            self.outside_text.append(value)


def _content(pieces: list[Piece]) -> bool:
    return any(piece.kind == "image" or piece.value.strip() for piece in pieces)


def _blocks(markup: NoticeMarkup, region: Region) -> _Pairs:
    pairs = _Pairs(markup, region)
    for line in lines(markup.pieces):
        pairs.accept(line)
    pairs.finish("end_of_document")
    if not pairs.result:
        pairs.issues.add("no_paired_changes")
    return pairs


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
    _one(inner, ".contents.sw-Txtarea")
    markup = NoticeMarkup(html)
    markup.feed(html)
    markup.close()
    if markup.bodies != 1:
        raise ValueError("Errata raw body container is ambiguous")
    pairs = _blocks(markup, region)
    date_raw, parsed_date = _heading_date(inner, region)
    return StagedNotice(
        region,
        _cards(inner, region),
        tuple(pairs.result),
        date_raw,
        parsed_date,
        tuple(sorted(pairs.issues)),
        tuple(pairs.outside_text),
    )
