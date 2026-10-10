"""Exact source evidence and typed occurrences for four-layer build boundaries."""

from itertools import pairwise
from typing import TYPE_CHECKING, Annotated, Literal, Self

from pydantic import Field, model_validator

from sve_carddb.contracts.four_layer import (
    Bound,
    CardNameReference,
    Code,
    Constant,
    Expression,
    GlossaryReference,
    Hash,
    Id,
    OccurrenceKey,
    OwnerField,
    QuantityExpr,
    QuantitySpec,
    Reference,
    Role,
    Span,
    VocabularyReference,
    disjoint,
    hash_payload,
)
from sve_carddb.contracts.n0 import VERSION as N0_VERSION
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import RecordData, UInt

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping

    from sve_carddb.contracts.four_layer import Frame, LeafSlot

TypedValue = (
    UInt | bool | str | Reference | tuple[Code, ...] | QuantitySpec | QuantityExpr
)


class ZoneDomain(RecordData):
    type: Literal["ZoneSet"]
    zones: Annotated[tuple[Code, ...], Field(min_length=1)]


class QuantityDomain(RecordData):
    type: Literal["QuantitySpec", "QuantityExpr"]
    modes: tuple[Literal["exact", "up_to", "at_least", "all", "any"], ...]
    imports: tuple[Code, ...]
    expressions: tuple[Code, ...]
    constant_only: bool = False


class ReferenceDomain(RecordData):
    type: Literal["Concept", "CardName", "CardKind"]
    category: Literal[
        "keyword", "ability", "rule_term", "trait", "class", "card_name", "type"
    ]
    references: tuple[Reference, ...]

    @model_validator(mode="after")
    def _catalog(self) -> Self:
        expected = {"CardName": {"card_name"}, "CardKind": {"type"}}.get(
            self.type, {"keyword", "ability", "rule_term", "trait", "class"}
        )
        if self.category not in expected:
            raise ValueError("Reference domain has the wrong catalog category")
        encoded = tuple(canonical(r.model_dump(mode="json")) for r in self.references)
        if encoded != tuple(sorted(set(encoded))):
            raise ValueError("Reference catalog must be sorted and unique")
        for reference in self.references:
            if self.category == "card_name":
                valid = isinstance(reference, CardNameReference)
            elif self.category in {"class", "type"}:
                valid = (
                    isinstance(reference, VocabularyReference)
                    and reference.key[0] == self.category
                )
            else:
                valid = isinstance(reference, GlossaryReference)
            if not valid:
                raise ValueError("Reference domain contains the wrong reference kind")
        return self


class LayoutDomain(RecordData):
    type: Literal["LiteralLayout"]


ClosedDomain = ZoneDomain | QuantityDomain | ReferenceDomain | LayoutDomain


class SourceReference(RecordData):
    batch_id: Id
    source_version_id: Id
    parser: Id
    locator: Annotated[str, Field(pattern=r"^/")]
    text_hash: Hash


class SourceDescriptor(OwnerField):
    source_unit_id: Id
    source_hash: Hash
    source_ref: SourceReference

    @model_validator(mode="after")
    def _linked(self) -> Self:
        if self.source_ref.text_hash != self.source_hash:
            raise ValueError("Source reference hash differs from exact field")
        return self

    def verify(self, expected: SourceDescriptor, raw: str) -> None:
        """An owner resolver supplies exact identity; equal text never substitutes for it."""
        if self != expected:
            raise ValueError("Source descriptor does not belong to exact owner field")
        if digest(raw.encode())[7:] != self.source_hash:
            raise ValueError("Source descriptor has stale exact bytes")


class SourceSpan(RecordData):
    role: Role
    segments: Annotated[tuple[Span, ...], Field(min_length=1)]
    anchor: UInt | None

    @model_validator(mode="after")
    def _ordered(self) -> Self:
        disjoint(self.segments)
        if self.role != "reminder" and self.anchor is not None:
            raise ValueError("Only reminder source may have an anchor")
        return self


class LeafOccurrence(RecordData):
    slot: Code
    ordinal: UInt
    raw_spans: tuple[Span, ...]
    canonical_spans: tuple[Span, ...]
    source_unit: str | None
    source_presence: Literal["explicit", "omitted"]
    resolution_rule: Code | None

    @model_validator(mode="after")
    def _ordered(self) -> Self:
        disjoint(self.raw_spans)
        disjoint(self.canonical_spans)
        if self.source_presence == "omitted":
            if self.raw_spans or self.canonical_spans:
                raise ValueError("Omitted leaf cannot fabricate source spans")
        elif not self.raw_spans or not self.canonical_spans:
            raise ValueError("Explicit leaf requires source and canonical spans")
        return self


class TracePiece(RecordData):
    raw_span: Span
    canonical_spans: tuple[Span, ...]
    rule: Code

    @model_validator(mode="after")
    def _ordered(self) -> Self:
        disjoint(self.canonical_spans)
        return self


def verify_partition(raw: str, spans: tuple[SourceSpan, ...]) -> None:
    """Complete coverage includes exact layout bytes and excludes semantic whitespace repairs."""
    ordered = sorted(
        ((s, part.role) for part in spans for s in part.segments),
        key=lambda item: item[0].start,
    )
    cursor = 0
    for span, role in ordered:
        if span.start != cursor or span.end > len(raw):
            raise ValueError("Source partition has a gap, overlap or invalid bound")
        if role == "layout" and not raw[span.start : span.end].isspace():
            raise ValueError("Layout cannot absorb source semantics")
        cursor = span.end
    if cursor != len(raw):
        raise ValueError("Source partition does not cover complete field")
    first = tuple(part.segments[0].start for part in spans)
    if first != tuple(sorted(first)):
        raise ValueError("Source bindings must follow their first segment")
    _verify_anchors(raw, spans)


def _verify_anchors(raw: str, spans: tuple[SourceSpan, ...]) -> None:
    lines = tuple(raw[: part.segments[0].start].count("\n") for part in spans)
    for index, part in enumerate(spans):
        if part.anchor is not None:
            if (
                part.anchor >= len(spans)
                or spans[part.anchor].role != "body"
                or lines[part.anchor] != lines[index]
            ):
                raise ValueError(
                    "Reminder anchor must identify its source-line body ordinal"
                )
        elif part.role == "reminder" and any(
            p.role == "body" and lines[i] == lines[index] for i, p in enumerate(spans)
        ):
            raise ValueError(
                "Inline reminder must anchor to its source-line body ordinal"
            )


def verify_trace(
    raw: str,
    canonical_source: str,
    span: SourceSpan,
    trace: tuple[TracePiece, ...],
    rules: Mapping[str, Callable[[str], str]],
) -> None:
    """Reapply pinned rules to raw evidence before accepting normalized coordinates."""
    disjoint(tuple(piece.raw_span for piece in trace))
    actual = tuple(
        i for piece in trace for i in range(piece.raw_span.start, piece.raw_span.end)
    )
    expected = tuple(
        i for segment in span.segments for i in range(segment.start, segment.end)
    )
    if actual != expected:
        raise ValueError("Trace does not cover exact source segments")
    covered: set[int] = set()
    for piece in trace:
        if piece.rule not in rules or piece.raw_span.end > len(raw):
            raise ValueError("Unknown trace rule or source bound")
        if any(s.end > len(canonical_source) for s in piece.canonical_spans):
            raise ValueError("Trace canonical span is out of bounds")
        source = raw[piece.raw_span.start : piece.raw_span.end]
        rebuilt = rules[piece.rule](source)
        projected = "".join(
            canonical_source[s.start : s.end] for s in piece.canonical_spans
        )
        if rebuilt != projected:
            raise ValueError("Trace rule does not reconstruct canonical bytes")
        covered.update(i for s in piece.canonical_spans for i in range(s.start, s.end))
    if covered != set(range(len(canonical_source))):
        raise ValueError("Trace does not reconstruct complete canonical source")


def verify_value(
    slot: LeafSlot,
    value: TypedValue,
    domains: Mapping[str, ClosedDomain] | None = None,
) -> None:
    """A source value is accepted according to its declared leaf type, never renderer guesses."""
    if slot.type in {"Nat", "Ordinal"}:
        _numeric_value(slot, value)
    elif slot.type in {"ZoneSet", "QuantitySpec", "QuantityExpr", "LiteralLayout"} or (
        len(slot.domain.values) == 1 and isinstance(slot.domain.values[0], str)
    ):
        _complex_value(slot, value, domains or {})
    elif value not in slot.domain.values:
        raise ValueError("Leaf value is outside its source domain")


def _numeric_value(slot: LeafSlot, value: TypedValue) -> None:
    if type(value) is not int or slot.domain.min is None or slot.domain.max is None:
        raise ValueError("Numeric leaf requires a safe integer value")
    if not slot.domain.min <= value <= slot.domain.max:
        raise ValueError("Numeric leaf is outside its source domain")


def _zone_value(domain: ZoneDomain, value: TypedValue) -> None:
    if not isinstance(value, tuple) or not value or value != tuple(sorted(set(value))):
        raise ValueError("ZoneSet requires sorted unique zone codes")
    if any(v not in domain.zones for v in value):
        raise ValueError("ZoneSet value is outside its source domain")


def _complex_value(
    slot: LeafSlot, value: TypedValue, domains: Mapping[str, ClosedDomain]
) -> None:
    registered = []
    for name in slot.domain.values:
        if (
            not isinstance(name, str)
            or name not in domains
            or domains[name].type != slot.type
        ):
            raise ValueError("Unknown or incorrectly typed named leaf domain")
        registered.append(domains[name])
    for domain in registered:
        try:
            _registered_value(domain, value)
        except ValueError:
            continue
        return
    raise ValueError("Leaf value is outside every registered source domain")


def _registered_value(domain: ClosedDomain, value: TypedValue) -> None:
    if isinstance(domain, ZoneDomain):
        _zone_value(domain, value)
    elif isinstance(domain, QuantityDomain):
        _quantity_value(domain, value)
    elif isinstance(domain, ReferenceDomain):
        if value not in domain.references:
            raise ValueError("Reference is outside the active adopted catalog")
    elif not isinstance(value, str) or not value.isspace():
        raise ValueError("LiteralLayout must contain only source whitespace")


def _quantity_value(domain: QuantityDomain, value: TypedValue) -> None:
    if domain.type == "QuantitySpec":
        if not isinstance(value, QuantitySpec) or value.mode not in domain.modes:
            raise ValueError("QuantitySpec value is outside registered source domain")
        expr = value.expr
    else:
        if not isinstance(value, (Bound, Constant, Expression)):
            raise ValueError("QuantityExpr leaf has the wrong value shape")
        expr = value
    if domain.constant_only and not isinstance(expr, Constant):
        raise ValueError("Quantity domain requires a constant expression")
    if isinstance(expr, Bound) and expr.import_ not in domain.imports:
        raise ValueError("Quantity expression refers to unknown import")
    if isinstance(expr, Expression) and expr.expression_id not in domain.expressions:
        raise ValueError("Quantity expression refers to unknown expression")


class SourceBinding(RecordData):
    id: Annotated[str, Field(pattern=r"^bind:[0-9a-f]{64}\Z")]
    source: SourceDescriptor
    ordinal: UInt
    line_ordinal: UInt
    frame_id: Annotated[str, Field(pattern=r"^frame:[0-9a-f]{64}\Z")]
    source_span: SourceSpan
    values: dict[Code, TypedValue]
    occurrences: tuple[LeafOccurrence, ...]
    trace: Annotated[tuple[TracePiece, ...], Field(min_length=1)]

    def occurrence_key(self) -> OccurrenceKey:
        """Pending frame identity remains independent of the eventual frame key."""
        return OccurrenceKey(
            owner=self.source.owner,
            field=self.source.field,
            ordinal=self.source.ordinal,
            source_hash=self.source.source_hash,
            line_ordinal=self.line_ordinal,
            role=self.source_span.role,
            segments=self.source_span.segments,
        )

    def payload(self) -> dict[str, object]:
        """Archive batch labels do not change already verified binding semantics."""
        return {
            "recipe": "binding-v2",
            "occurrence": self.occurrence_key().model_dump(mode="json"),
            **self.model_dump(
                mode="json",
                by_alias=True,
                include={"frame_id", "source_span", "values", "occurrences", "trace"},
            ),
        }

    def verify(
        self, frame: Frame, domains: Mapping[str, ClosedDomain] | None = None
    ) -> None:
        """Required leaves and every repeated source occurrence survive binding validation."""
        if self.frame_id != frame.id or self.source_span.role != frame.role:
            raise ValueError("Binding frame or source role mismatch")
        if frame.source.normalizer_version == N0_VERSION and (
            any(
                occurrence.source_presence != "explicit"
                for occurrence in self.occurrences
            )
            or any(
                isinstance(value, QuantitySpec) and not isinstance(value.expr, Constant)
                for value in self.values.values()
            )
        ):
            raise ValueError("N0 requires explicit leaves and constant quantities")
        if (
            frame.semantic_variant.scope is not None
            and frame.semantic_variant.scope != self.occurrence_key()
        ):
            raise ValueError("Pending frame belongs to another exact occurrence")
        slots = {s.name: s for s in frame.leaf_schema.slots}
        if set(self.values) - slots.keys() or any(
            s.required and s.name not in self.values for s in slots.values()
        ):
            raise ValueError("Binding has unknown or missing required leaf values")
        for name, value in self.values.items():
            verify_value(slots[name], value, domains)
            _quantity_links(frame, value)
        _verify_occurrences(self.occurrences, slots, self.values)
        if self.id != "bind:" + hash_payload(self.payload()):
            raise ValueError("Binding identity does not match verified payload")


def _quantity_links(frame: Frame, value: TypedValue) -> None:
    expr = value.expr if isinstance(value, QuantitySpec) else value
    if isinstance(expr, Bound):
        imports = {p.name: p for p in frame.projection.imports}
        if expr.import_ not in imports or imports[expr.import_].type not in {
            "QuantityExpr",
            "CapturedValue",
        }:
            raise ValueError("Quantity expression has a missing or mistyped import")
    if isinstance(expr, Expression):
        slots = {s.name for s in frame.leaf_schema.slots}
        if any(name not in slots for name in expr.leaves):
            raise ValueError("Quantity expression refers to an unknown leaf")


def _verify_occurrences(
    occurrences: tuple[LeafOccurrence, ...],
    slots: Mapping[str, LeafSlot],
    values: Mapping[str, TypedValue],
) -> None:
    order = tuple(
        (o.raw_spans[0].start, o.slot)
        for o in occurrences
        if o.source_presence == "explicit"
    )
    if order != tuple(sorted(order)):
        raise ValueError("Leaf occurrences must follow raw source order and slot")
    for name, slot in slots.items():
        found = tuple(o for o in occurrences if o.slot == name)
        if (
            name in values
            and not slot.occurrences
            and (len(found) != 1 or found[0].source_presence != "omitted")
        ):
            raise ValueError("Omitted value requires its own source occurrence")
        if tuple(o.ordinal for o in found) != tuple(range(len(found))):
            raise ValueError("Leaf occurrence ordinals must be continuous")
        if tuple(s for o in found for s in o.canonical_spans) != slot.occurrences:
            raise ValueError(
                "Leaf occurrences do not cover declared canonical positions"
            )
        if name in values and any(
            o.source_presence == "omitted" and o.resolution_rule is None for o in found
        ):
            raise ValueError("Omitted leaf value requires a named resolution rule")
    if any(o.slot not in slots for o in occurrences):
        raise ValueError("Leaf occurrence references an unknown slot")
    raw = sorted((s for o in occurrences for s in o.raw_spans), key=lambda s: s.start)
    if any(a.end > b.start for a, b in pairwise(raw)):
        raise ValueError("Leaf raw occurrences overlap")
