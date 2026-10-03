"""Candidate anchors plus an independent full-field UTF-8 partition verifier."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.template_parameters.models import Range, SourceSpan

if TYPE_CHECKING:
    from sve_carddb.template_sources.normalizer import Part


@dataclass(frozen=True)
class Located:
    ordinal: int
    line_ordinal: int
    source_span: SourceSpan


def locate(text: str, parts: tuple[Part, ...]) -> tuple[Located, ...]:
    """Use legacy roles as candidates, without adopting the bracket classification."""
    bodies = {
        part.line_ordinal: ordinal
        for ordinal, part in enumerate(parts)
        if part.role == "body"
    }
    if len(bodies) != sum(part.role == "body" for part in parts):
        raise ValueError("A source line must not contain multiple body candidates")
    located = tuple(
        Located(
            ordinal,
            part.line_ordinal,
            SourceSpan(
                role=part.role,
                segments=tuple(Range(start=s.start, end=s.end) for s in part.segments),
                anchor=bodies.get(part.line_ordinal)
                if part.role == "reminder"
                else None,
            ),
        )
        for ordinal, part in enumerate(parts)
    )
    verify(text, located)
    return located


def verify(text: str, located: tuple[Located, ...]) -> None:
    """Reassemble raw bytes by source positions, independently of the normalizer."""
    if [item.ordinal for item in located] != list(range(len(located))):
        raise ValueError("Source binding ordinals must be continuous from zero")
    if [item.source_span.segments[0].start for item in located] != sorted(
        item.source_span.segments[0].start for item in located
    ):
        raise ValueError("Source bindings must follow their first source position")
    spans = sorted(
        (s for item in located for s in item.source_span.segments),
        key=lambda s: s.start,
    )
    position = 0
    fragments = []
    for span in spans:
        if span.start != position or span.end > len(text):
            raise ValueError("Source bindings must partition every raw code point")
        fragments.append(text[span.start : span.end].encode())
        position = span.end
    if position != len(text) or b"".join(fragments) != text.encode():
        raise ValueError("Source bindings must roundtrip exact field UTF-8 bytes")
    for item in located:
        _verify_anchor(text, item, located)


def _verify_anchor(text: str, item: Located, located: tuple[Located, ...]) -> None:
    source_span = item.source_span
    if item.line_ordinal != text[: source_span.segments[0].start].count("\n"):
        raise ValueError("Source binding line must agree with its raw position")
    if source_span.role == "layout" and not all(
        text[s.start : s.end].isspace() for s in source_span.segments
    ):
        raise ValueError("Layout candidates may contain only source whitespace")
    if source_span.anchor is not None:
        if not 0 <= source_span.anchor < len(located):
            raise ValueError("Reminder anchor must locate a body in the same line")
        target = located[source_span.anchor]
        if (
            target.source_span.role != "body"
            or target.line_ordinal != item.line_ordinal
        ):
            raise ValueError("Reminder anchor must locate a body in the same line")
    elif source_span.role == "reminder" and any(
        other.line_ordinal == item.line_ordinal and other.source_span.role == "body"
        for other in located
    ):
        raise ValueError("Inline reminder must anchor to its source-line body")
