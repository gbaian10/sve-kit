"""Source coordinates and typed context wiring shared by the P2 producers."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    ContextExpression,
    Expression,
    LeafSchema,
    OccurrenceKey,
    Port,
    PortEvidence,
    PredicateExpr,
    Projection,
    Scope,
    Span,
)
from sve_carddb.domains.translations.four_layer_n1_leaves import Leaf
from sve_carddb.domains.translations.four_layer_n1_transform import transform
from sve_carddb.domains.translations.recognition.provenance import merged

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import CaptureTime, PortValue
    from sve_carddb.contracts.source_binding import SourceDescriptor, TypedValue
    from sve_carddb.domains.translations.four_layer_classification import (
        Classifier,
        Recognized,
    )
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart


def origins(part: SourcePart, span: Span) -> tuple[Span, ...]:
    """Retain raw coordinates across normalized source units."""
    return merged(
        tuple(s for u in part.units[span.start : span.end] for s in u.origins)
    )


def spelling(raw: str, part: SourcePart, span: Span) -> str:
    """Read only this exact occurrence, including normalized numeric spellings."""
    return "".join(raw[s.start : s.end] for s in origins(part, span))


def occurrence(source: SourceDescriptor, part: SourcePart) -> OccurrenceKey:
    """Anchor imports to the same owner, field and source revision as the binding."""
    return OccurrenceKey(
        owner=source.owner,
        field=source.field,
        ordinal=source.ordinal,
        source_hash=source.source_hash,
        line_ordinal=part.line_ordinal,
        role=part.source_span.role,
        segments=part.source_span.segments,
    )


def context_port(
    name: str,
    role: str,
    scope: str,
    value: PortValue,
    capture: CaptureTime,
    source: OccurrenceKey,
) -> Port:
    """Keep capture and type explicit at the shared context boundary."""
    return Port(
        name=name,
        type=value.kind,
        source_role=role,
        scope=scope,
        evidence=PortEvidence(source=source, capture=capture, value=value),
    )


def source_scope(text: str, start: int, end: int, consumer: str) -> str:
    """Granted text and numbered branches cannot borrow their enclosing query context."""
    stack: list[int] = []
    for index, char in enumerate(text):
        if char == "「":
            stack.append(index)
        elif char == "」" and stack:
            opened = stack.pop()
            if (
                opened < start < end <= index
                and re.match(r"「【(?:攻撃時|進化時)】", text[opened:])
                and text[index:].startswith("」を持つ")
            ):
                return f"p2.granted_{opened}.{consumer}"
    heads = tuple(
        re.finditer(
            r"\{(?:起動|進化|ファンファーレ|ラストワード)\}|【(?:攻撃時|進化時)】",
            text[:start],
        )
    )
    ability = heads[-1].start() if heads else 0
    modes = tuple(re.finditer(r"【[0-9]+】", text[ability:start]))
    branch = f".mode_{modes[-1].start()}" if modes else ""
    return f"p2.ability_{ability}{branch}.{consumer}"


def interface(
    old: Projection | None, scopes: tuple[str, ...], ports: tuple[Port, ...]
) -> Projection:
    """Add independently scoped source providers without resolving whole-line semantics."""
    base = old or Projection(
        projection_kind="pending", discriminator=None, scopes=(), imports=(), exports=()
    )
    known = {s.id: s for s in base.scopes}
    for name in scopes:
        # Separate ability roots deliberately prevent a granted read from escaping.
        known[name] = Scope(id=name, parent=None, kind="ability")
    return base.model_copy(
        update={
            "scopes": tuple(sorted(known.values(), key=lambda s: s.id)),
            "imports": tuple(sorted((*base.imports, *ports), key=lambda p: p.name)),
        }
    )


def _project(original: SourcePart, current: SourcePart, span: Span) -> tuple[Span, ...]:
    raw = origins(original, span)
    return merged(
        tuple(
            Span(start=i, end=i + 1)
            for i, unit in enumerate(current.units)
            if any(
                a.start < b.end and b.start < a.end for a in unit.origins for b in raw
            )
        )
    )


def update_source(
    raw: str,
    original: SourcePart,
    current: SourcePart,
    found: Recognized,
    additions: list[Leaf],
    classifier: Classifier,
) -> tuple[SourcePart, Recognized, tuple[str, ...], dict[str, str]]:
    """Preserve raw provenance while renaming the complete expression DAG together."""
    projected = [
        replace(
            item,
            positions=tuple(
                tuple(p for span in group for p in _project(original, current, span))
                for group in item.positions
            ),
            slot=item.slot.model_copy(
                update={
                    "occurrences": merged(
                        tuple(
                            p
                            for group in item.positions
                            for span in group
                            for p in _project(original, current, span)
                        )
                    )
                }
            ),
        )
        for item in additions
    ]
    replaced = tuple(s for item in projected for s in item.slot.occurrences)
    leaves = []
    for slot in found.schema.slots:
        if any(
            a.start < b.end and b.start < a.end
            for a in slot.occurrences
            for b in replaced
        ):
            continue
        uses = tuple(o for o in found.occurrences if o.slot == slot.name)
        leaves.append(
            Leaf(
                slot,
                found.values[slot.name],
                tuple(o.canonical_spans for o in uses if o.canonical_spans),
                "N1",
                abstract=any(
                    u.transformation == "leaf"
                    for span in slot.occurrences
                    for u in current.units[span.start : span.end]
                ),
                unit=uses[0].source_unit,
                resolution_rule=uses[0].resolution_rule,
                use=next((s.start for o in uses for s in o.raw_spans), 0),
            )
        )
    leaves.extend(projected)
    covered = merged(
        tuple(
            s
            for item in leaves
            for group in item.positions
            for span in group
            for s in origins(current, span)
        )
    )
    issues = set(found.issues)
    candidate = classifier.candidate(raw, original)
    for reason in (
        "numeric_role_requires_review",
        "signed_numeric_requires_review",
        "n0_numeric_construction_unresolved",
        "n0_vocabulary_construction_unresolved",
    ):
        hints = tuple(
            h
            for h in candidate.slots
            if reason in h.issues
            or (reason == "n0_numeric_construction_unresolved" and h.type == "uint")
            or (
                reason == "n0_vocabulary_construction_unresolved"
                and h.target
                and h.target.get("kind") == "vocabulary"
            )
        )
        if hints and all(
            any(a.start <= b.start < b.end <= a.end for a in covered)
            for h in hints
            for b in h.source_segments
        ):
            issues.discard(reason)
    part, result, rows = transform(
        raw,
        current,
        leaves,
        (),
        replace(found, issues=tuple(sorted(issues))),
        separate_occurrences=True,
    )
    names = {item.slot.name: f"leaf_{i}" for i, item in enumerate(leaves)}

    def remap(value: TypedValue) -> TypedValue:
        if isinstance(value, (Expression, ContextExpression, PredicateExpr)):
            return value.model_copy(
                update={"leaves": tuple(names[n] for n in value.leaves)}
            )
        return value

    values = {name: remap(value) for name, value in result.values.items()}
    schema = LeafSchema(
        format=2,
        slots=tuple(
            slot.model_copy(update={"expression": values[slot.name]})
            if isinstance(values[slot.name], (ContextExpression, PredicateExpr))
            else slot
            for slot in result.schema.slots
        ),
    )
    return part, replace(result, values=values, schema=schema), rows, names
