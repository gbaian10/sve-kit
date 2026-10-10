"""Pinned source partitions and replayable Unicode provenance for four-layer bindings."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal
from unicodedata import normalize

from sve_carddb.contracts.four_layer import Constant, QuantitySpec, Span
from sve_carddb.contracts.n0 import VERSION
from sve_carddb.contracts.source_binding import SourceSpan, TracePiece, verify_partition
from sve_carddb.contracts.template_parameters import Range
from sve_carddb.core.json import digest
from sve_carddb.domains.translations.parameters.provenance import Unit, merged, nfkc
from sve_carddb.domains.translations.source_inventory.normalizer import TOKEN_HEADER

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.contracts.four_layer import Frame, Role
    from sve_carddb.contracts.source_binding import (
        ClosedDomain,
        SourceBinding,
        SourceDescriptor,
    )

__all__ = ("VERSION", "SourceField", "SourcePart", "normalize_source", "replay_part")
_NAME = re.compile(r"『[^』]*』")
_REMINDER = re.compile(r"（[^（）]*）")
_NUMBER = re.compile(r"[0-9０-９]+")


@dataclass(frozen=True)
class SourcePart:
    ordinal: int
    line_ordinal: int
    source_span: SourceSpan
    canonical_source: str
    trace: tuple[TracePiece, ...]
    units: tuple[Unit, ...]

    def verify(
        self,
        raw: str,
        frame: Frame,
        binding: SourceBinding,
        domains: Mapping[str, ClosedDomain],
    ) -> None:
        """Only replayed source coordinates authorize an authored frame or stored binding."""
        if frame.source.normalizer_version != VERSION:
            raise ValueError("Unsupported four-layer normalizer version")
        if (
            binding.ordinal != self.ordinal
            or binding.line_ordinal != self.line_ordinal
            or binding.source_span != self.source_span
            or binding.trace != self.trace
            or frame.role != self.source_span.role
        ):
            raise ValueError("Binding differs from replayed exact source partition")
        frame.verify(self.canonical_source)
        binding.verify(frame, domains)
        self._verify_replacements(frame)
        slots = {slot.name: slot for slot in frame.leaf_schema.slots}
        for occurrence in binding.occurrences:
            origins = merged(
                tuple(
                    origin
                    for span in occurrence.canonical_spans
                    for unit in self.units[span.start : span.end]
                    for origin in unit.origins
                )
            )
            expected = tuple(Span(start=s.start, end=s.end) for s in origins)
            if occurrence.raw_spans != expected:
                raise ValueError("Leaf raw positions differ from replayed provenance")
            if occurrence.source_presence == "explicit":
                value = binding.values.get(occurrence.slot)
                spelling = "".join(raw[s.start : s.end] for s in expected)
                numeric = (
                    value.expr.value
                    if isinstance(value, QuantitySpec)
                    and isinstance(value.expr, Constant)
                    else value
                )
                if slots[occurrence.slot].type in {
                    "Nat",
                    "Ordinal",
                    "QuantitySpec",
                } and (
                    _NUMBER.fullmatch(spelling) is None
                    or numeric != int(normalize("NFKC", spelling))
                ):
                    raise ValueError("Numeric leaf value differs from exact source")
                if slots[occurrence.slot].type == "LiteralLayout" and value != spelling:
                    raise ValueError("Layout leaf value differs from exact source")

    def _verify_replacements(self, frame: Frame) -> None:
        for index, unit in enumerate(self.units):
            expected = (
                {"LiteralLayout"}
                if self.source_span.role == "layout"
                else {"CardName", "Concept"}
                if unit.transformation == "quoted"
                else {"Nat", "Ordinal", "QuantitySpec", "QuantityExpr"}
                if unit.transformation == "digits"
                else None
            )
            if expected is not None and not any(
                slot.type in expected
                and slot.required
                and any(s.start <= index < s.end for s in slot.occurrences)
                for slot in frame.leaf_schema.slots
            ):
                raise ValueError(
                    "Normalized source replacement requires its typed leaf"
                )


@dataclass(frozen=True)
class SourceField:
    source: SourceDescriptor
    parts: tuple[SourcePart, ...]
    reminders: frozenset[str]

    def verify(
        self,
        raw: str,
        frames: tuple[Frame, ...],
        bindings: tuple[SourceBinding, ...],
        domains: Mapping[str, ClosedDomain],
    ) -> None:
        """Every owner use replays its own field, even when its context is shared."""
        if len(frames) != len(self.parts) or len(bindings) != len(self.parts):
            raise ValueError("Bindings must cover the complete exact source field")
        if normalize_source(raw, self.source, reminders=self.reminders) != self:
            raise ValueError("Source field differs from pinned normalization replay")
        for part, frame, binding in zip(self.parts, frames, bindings, strict=True):
            if binding.source != self.source:
                raise ValueError("Binding borrows another owner source descriptor")
            part.verify(raw, frame, binding, domains)


def replay_part(
    raw: str,
    frame: Frame,
    binding: SourceBinding,
    domains: Mapping[str, ClosedDomain],
) -> None:
    """Typed DB reads recheck a part's exact bytes; complete classification still belongs to B."""
    binding.source.verify(binding.source, raw)
    span = binding.source_span
    if any(segment.end > len(raw) for segment in span.segments):
        raise ValueError("Stored source part is outside exact field")
    named_role = (
        "name"
        if binding.source.field == "name"
        else "label"
        if binding.source.field in {"label", "action_label"}
        else None
    )
    if named_role is not None and span.role != named_role:
        raise ValueError("Stored source part has the wrong named-field role")
    if raw[: span.segments[0].start].count("\n") != binding.line_ordinal:
        raise ValueError("Stored source part has the wrong raw line ordinal")
    units = _units(raw, span)
    part = SourcePart(
        binding.ordinal,
        binding.line_ordinal,
        span,
        "".join(unit.text for unit in units),
        _trace(raw, span, units),
        units,
    )
    part.verify(raw, frame, binding, domains)


def normalize_source(
    raw: str, source: SourceDescriptor, *, reminders: frozenset[str] = frozenset()
) -> SourceField:
    """Preserve source bytes before applying a pinned, finite normalization recipe."""
    if digest(raw.encode())[7:] != source.source_hash:
        raise ValueError("Normalizer source has stale exact bytes")
    spans: tuple[SourceSpan, ...]
    if source.field in {"name", "label", "action_label"}:
        spans = (
            (
                SourceSpan(
                    role="name" if source.field == "name" else "label",
                    segments=(Span(start=0, end=len(raw)),),
                    anchor=None,
                ),
            )
            if raw
            else ()
        )
    else:
        spans = _partition(raw, source.field == "section", reminders)
    verify_partition(raw, spans)
    parts = []
    for ordinal, span in enumerate(spans):
        units = _units(raw, span)
        parts.append(
            SourcePart(
                ordinal,
                raw[: span.segments[0].start].count("\n"),
                span,
                "".join(unit.text for unit in units),
                _trace(raw, span, units),
                units,
            )
        )
    return SourceField(source, tuple(parts), reminders)


def _segments(positions: tuple[int, ...]) -> tuple[Span, ...]:
    result: list[Span] = []
    for position in positions:
        if result and result[-1].end == position:
            result[-1] = Span(start=result[-1].start, end=position + 1)
        else:
            result.append(Span(start=position, end=position + 1))
    return tuple(result)


def _line(
    line: str, offset: int, section: bool, reminders: frozenset[str]
) -> tuple[SourceSpan, ...]:
    roles: list[Role] = ["layout"] * len(line)
    header = TOKEN_HEADER.match(line) if section else None
    header_end = header.end() if header is not None else 0
    roles[:header_end] = ["token_header"] * header_end
    names = tuple(_NAME.finditer(line))
    start = header_end + len(line[header_end:]) - len(line[header_end:].lstrip())
    stop = len(line.rstrip())
    reminder_spans = tuple(
        (start + m.start(), start + m.end())
        for m in _REMINDER.finditer(line[start:stop])
        if m.group() in reminders
        and not any(n.start() <= start + m.start() < n.end() for n in names)
    )
    for first, last in reminder_spans:
        roles[first:last] = ["reminder"] * (last - first)
    kept = tuple(i for i in range(start, stop) if roles[i] != "reminder")
    body = "".join(line[i] for i in kept)
    left, right = len(body) - len(body.lstrip()), len(body.rstrip())
    kept = kept[left:right]
    for i in kept:
        roles[i] = "body"
    result = []
    if kept:
        result.append(
            SourceSpan(
                role="body",
                segments=_segments(tuple(offset + i for i in kept)),
                anchor=None,
            )
        )
    result.extend(
        SourceSpan(
            role="reminder",
            segments=(Span(start=offset + first, end=offset + last),),
            anchor=None,
        )
        for first, last in reminder_spans
    )
    for role in ("token_header", "layout"):
        result.extend(
            SourceSpan(role=role, segments=(s,), anchor=None)
            for s in _segments(
                tuple(offset + i for i, actual in enumerate(roles) if actual == role)
            )
        )
    return tuple(result)


def _partition(
    raw: str, section: bool, reminders: frozenset[str]
) -> tuple[SourceSpan, ...]:
    spans: list[SourceSpan] = []
    offset = 0
    for line in raw.split("\n"):
        spans.extend(_line(line, offset, section, reminders))
        offset += len(line)
        if offset < len(raw):
            spans.append(
                SourceSpan(
                    role="layout",
                    segments=(Span(start=offset, end=offset + 1),),
                    anchor=None,
                )
            )
            offset += 1
    spans.sort(key=lambda p: p.segments[0].start)
    body_lines = {
        raw[: p.segments[0].start].count("\n"): ordinal
        for ordinal, p in enumerate(spans)
        if p.role == "body"
    }
    return tuple(
        p.model_copy(
            update={"anchor": body_lines.get(raw[: p.segments[0].start].count("\n"))}
        )
        if p.role == "reminder"
        else p
        for p in spans
    )


def _units(raw: str, span: SourceSpan) -> tuple[Unit, ...]:
    positions = tuple(i for s in span.segments for i in range(s.start, s.end))
    text = "".join(raw[i] for i in positions)
    if span.role == "layout":
        return (
            Unit("W", tuple(Range(start=s.start, end=s.end) for s in span.segments)),
        )
    if span.role != "body":
        return tuple(Unit(raw[i], (Range(start=i, end=i + 1),)) for i in positions)
    units = nfkc(text, positions)
    value = "".join(u.text for u in units)
    names = tuple(_NAME.finditer(value))
    replacements: tuple[tuple[int, int, Literal["quoted", "digits"]], ...] = tuple(
        (m.start() + 1, m.end() - 1, "quoted")
        for m in names
        if m.start() + 1 < m.end() - 1
    )
    replacements += tuple(
        (m.start(), m.end(), "digits")
        for m in _NUMBER.finditer(value)
        if not any(n.start() <= m.start() < n.end() for n in names)
    )
    result: list[Unit] = []
    cursor = 0
    for first, last, transformation in sorted(replacements):
        result.extend(units[cursor:first])
        origins = merged(tuple(s for u in units[first:last] for s in u.origins))
        result.append(
            Unit("X" if transformation == "quoted" else "N", origins, transformation)
        )
        cursor = last
    result.extend(units[cursor:])
    return tuple(result)


def _trace(
    raw: str, span: SourceSpan, units: tuple[Unit, ...]
) -> tuple[TracePiece, ...]:
    positions: dict[int, list[int]] = {}
    rules: dict[int, str] = {}
    for index, unit in enumerate(units):
        for origin in unit.origins:
            for position in range(origin.start, origin.end):
                positions.setdefault(position, []).append(index)
                rules[position] = (
                    "layout"
                    if span.role == "layout"
                    else unit.transformation
                    if unit.transformation != "literal"
                    else "identity"
                    if len(unit.origins) == 1
                    and unit.origins[0].end - unit.origins[0].start == 1
                    and unit.text == raw[position]
                    else "nfkc"
                )
    return tuple(
        TracePiece(
            raw_span=Span(start=i, end=i + 1),
            canonical_spans=_segments(tuple(positions[i])),
            rule=rules[i],
        )
        for s in span.segments
        for i in range(s.start, s.end)
    )
