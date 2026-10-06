"""Pinned v0 classification recipe, with an independent exact-source partition."""

import re
import unicodedata
from dataclasses import dataclass
from typing import Literal

from sve_carddb.snapshot.values import digest

VERSION = "classification-jp-v0-v1"
CODE_PATH = "carddb/src/sve_carddb/template_sources/normalizer.py"
TYPE_WORDS = "フォロワー|アミュレット|スペル|イクイップメント|クレスト"
TOKEN_HEADER = re.compile(
    r"^『(?P<name>[^』]+)』\{(?P<cls>[^}]+)\}(?P<kind>[^{『]*?(?:"
    + TYPE_WORDS
    + r"))(?:\{コスト(?P<cost>[^}]*)\})?"
    r"(?:\{攻撃力\}(?P<atk>[^/]*)/\{体力\}(?P<hp>[0-9０-９]+))?"
)
REMINDER = re.compile(r"（[^（）]*）")
DIGITS = re.compile(r"[0-9０-９]+")
QUOTED = re.compile(r"『[^』]*』")
type Role = Literal["body", "reminder", "token_header", "layout"]


@dataclass(frozen=True)
class Segment:
    start: int
    end: int


@dataclass(frozen=True)
class Part:
    line_ordinal: int
    role: Role
    segments: tuple[Segment, ...]
    normalized: str

    @property
    def normalized_hash(self) -> str:
        """Hash complete UTF-8 bytes, including literal N/X already in the source."""
        return digest(self.normalized.encode())


def normalize(body: str) -> str:
    """Apply the original transformations in order, without extra trimming."""
    text = unicodedata.normalize("NFKC", body)
    text = QUOTED.sub("『X』", text)
    return DIGITS.sub("N", text)


def _ranges(positions: list[int]) -> tuple[Segment, ...]:
    groups: list[Segment] = []
    for position in positions:
        if groups and groups[-1].end == position:
            groups[-1] = Segment(groups[-1].start, position + 1)
        else:
            groups.append(Segment(position, position + 1))
    return tuple(groups)


def _line(text: str, offset: int, ordinal: int, header_end: int) -> list[Part]:
    roles: list[Role] = ["layout"] * len(text)
    roles[:header_end] = ["token_header"] * header_end
    candidate = text[header_end:]
    lead = len(candidate) - len(candidate.lstrip())
    end = len(candidate.rstrip())
    start = header_end + lead
    stop = header_end + end
    reminders: list[Segment] = []
    for match in REMINDER.finditer(text[start:stop]):
        span = Segment(start + match.start(), start + match.end())
        reminders.append(span)
        roles[span.start : span.end] = ["reminder"] * (span.end - span.start)
    kept = [i for i in range(start, stop) if roles[i] != "reminder"]
    body = "".join(text[i] for i in kept)
    left = len(body) - len(body.lstrip())
    right = len(body.rstrip())
    kept = kept[left:right]
    for i in kept:
        roles[i] = "body"
    result = []
    if kept:
        result.append(
            Part(
                ordinal,
                "body",
                _ranges([offset + i for i in kept]),
                normalize("".join(text[i] for i in kept)),
            )
        )
    result.extend(
        Part(
            ordinal,
            "reminder",
            (Segment(offset + span.start, offset + span.end),),
            text[span.start : span.end],
        )
        for span in reminders
    )
    for role in ("token_header", "layout"):
        result.extend(
            Part(
                ordinal,
                role,
                (Segment(offset + span.start, offset + span.end),),
                text[span.start : span.end],
            )
            for span in _ranges([i for i, actual in enumerate(roles) if actual == role])
        )
    return result


def partition(text: str, *, section: int | None = None) -> tuple[Part, ...]:
    """Reproduce LF splitting/header-before-trim while retaining every moved byte."""
    result: list[Part] = []
    offset = 0
    for ordinal, line in enumerate(text.split("\n")):
        match = TOKEN_HEADER.match(line) if section is not None else None
        if (
            match is None
            and section is not None
            and line.startswith("『")
            and "』{" in line[:40]
        ):
            # Never include an unknown header's official text in an exception.
            raise ValueError("Unrecognized legacy token header")
        result.extend(_line(line, offset, ordinal, match.end() if match else 0))
        offset += len(line)
        if offset < len(text):
            result.append(Part(ordinal, "layout", (Segment(offset, offset + 1),), "\n"))
            offset += 1
    parts = tuple(sorted(result, key=lambda part: part.segments[0].start))
    verify_partition(text, parts)
    return parts


def verify_partition(text: str, parts: tuple[Part, ...]) -> None:
    """Check source coverage independently of normalization and unique fingerprints."""
    spans = sorted(
        (span for part in parts for span in part.segments), key=lambda s: s.start
    )
    position = 0
    raw = []
    for span in spans:
        if span.start != position or not span.start < span.end <= len(text):
            raise ValueError(
                "Legacy trace must partition every source code point exactly once"
            )
        raw.append(text[span.start : span.end])
        position = span.end
    if position != len(text) or "".join(raw).encode() != text.encode():
        raise ValueError("Legacy trace must roundtrip exact source UTF-8 bytes")
