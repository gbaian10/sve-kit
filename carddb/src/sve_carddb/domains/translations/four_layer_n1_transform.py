"""Transform the shared source units, preserving every raw origin through aliases."""

from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import LeafSchema, Span
from sve_carddb.contracts.source_binding import LeafOccurrence
from sve_carddb.domains.translations.four_layer_classification import Recognized
from sve_carddb.domains.translations.four_layer_normalizer import source_trace
from sve_carddb.domains.translations.recognition.provenance import Unit, merged

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.domains.translations.four_layer_n1 import Operand
    from sve_carddb.domains.translations.four_layer_n1_leaves import Leaf
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart

type Patches = dict[int, tuple[int, tuple[Unit, ...]]]


def _origins(part: SourcePart, spans: tuple[Span, ...]) -> tuple[Span, ...]:
    return merged(
        tuple(
            s
            for span in spans
            for u in part.units[span.start : span.end]
            for s in u.origins
        )
    )


def _union_patch(part: SourcePart, operand: Operand, patches: Patches) -> None:
    assert operand.noun is not None
    start, end = operand.span.start, operand.noun.span.start
    covered = {
        i
        for span in (*operand.owners, *operand.zone_spans)
        for i in range(span.start, span.end)
    }
    origins = merged(
        tuple(
            s
            for i in range(start, end)
            if i not in covered
            for s in part.units[i].origins
        )
    )
    fixed = (Unit("の", origins, "alias"),)
    units = (
        *patches[operand.owners[0].start][1],
        *fixed,
        *patches[operand.zone_spans[0].start][1],
        *fixed,
    )
    for key in tuple(patches):
        if start <= key < end:
            del patches[key]
    patches[start] = end, units


def _selection_patch(part: SourcePart, operand: Operand, patches: Patches) -> None:
    if operand.particle is not None:
        span = operand.particle
        patches[span.start] = span.end, (Unit("の", _origins(part, (span,)), "alias"),)
    if operand.pre_particle is not None:
        pre = operand.pre_particle
        patches[pre.start] = pre.end, ()
    if operand.number is None or operand.unit is None:
        return
    end = (
        operand.post_particle.end
        if operand.post_particle
        else operand.number.end
        + len(operand.unit)
        + (2 if operand.mode == "up_to" else 0)
    )
    origins = _origins(part, (operand.pre_particle,)) if operand.pre_particle else ()
    fixed = tuple(part.units[operand.number.end : end])
    if operand.post_particle is None:
        fixed += (Unit("を", origins, "alias"),)
    elif origins:
        fixed = (
            *fixed[:-1],
            Unit("を", merged((*fixed[-1].origins, *origins)), "alias"),
        )
    patches[operand.number.end] = end, fixed


def _patches(
    part: SourcePart,
    leaves: list[Leaf],
    operands: tuple[Operand, ...],
    names: Mapping[int, str],
) -> Patches:
    patches: Patches = {}
    for item in leaves:
        if not item.abstract or not item.positions:
            continue
        spans = tuple(span for group in item.positions for span in group)
        marker = tuple(
            Unit(char, _origins(part, spans), "leaf")
            for char in "{" + names[id(item)] + "}"
        )
        for span in spans:
            patches[span.start] = span.end, marker
    for operand in operands:
        if operand.row.startswith("N1-SRC10"):
            _union_patch(part, operand, patches)
        elif operand.row == "N1-SRC01.select":
            _selection_patch(part, operand, patches)
    return patches


def _units(part: SourcePart, patches: Patches) -> tuple[Unit, ...]:
    units: list[Unit] = []
    cursor = 0
    while cursor < len(part.units):
        if cursor in patches:
            stop, replacement = patches[cursor]
            units.extend(replacement)
            cursor = stop
        else:
            units.append(part.units[cursor])
            cursor += 1
    return tuple(units)


def _occurrences(
    part: SourcePart, units: tuple[Unit, ...], item: Leaf, name: str
) -> tuple[tuple[int, LeafOccurrence], ...]:
    if not item.positions:
        return (
            (
                item.use,
                LeafOccurrence(
                    slot=name,
                    ordinal=0,
                    raw_spans=(),
                    canonical_spans=(),
                    source_unit=None,
                    source_presence="omitted",
                    resolution_rule=item.resolution_rule,
                ),
            ),
        )
    result = []
    for ordinal, group in enumerate(item.positions):
        raw_spans = _origins(part, group)
        positions = [
            i
            for i, u in enumerate(units)
            if any(
                a.start < b.end and b.start < a.end
                for a in u.origins
                for b in raw_spans
            )
        ]
        canonical_spans = merged(tuple(Span(start=i, end=i + 1) for i in positions))
        result.append(
            (
                raw_spans[0].start,
                LeafOccurrence(
                    slot=name,
                    ordinal=ordinal,
                    raw_spans=raw_spans,
                    canonical_spans=canonical_spans,
                    source_unit=item.unit,
                    source_presence="explicit",
                    resolution_rule=None,
                ),
            )
        )
    return tuple(result)


def transform(
    raw: str,
    part: SourcePart,
    leaves: list[Leaf],
    operands: tuple[Operand, ...],
    base: Recognized,
) -> tuple[SourcePart, Recognized, tuple[str, ...]]:
    """Leaf markers are typed transformations, never recognized from literal source bytes."""
    leaves.sort(
        key=lambda item: (
            not item.slot.occurrences,
            item.slot.occurrences[0].start if item.slot.occurrences else item.use,
            item.slot.role,
        )
    )
    names = {id(item): f"leaf_{index}" for index, item in enumerate(leaves)}
    units = _units(part, _patches(part, leaves, operands, names))
    transformed = replace(
        part,
        units=units,
        canonical_source="".join(u.text for u in units),
        trace=source_trace(raw, part.source_span, units),
    )
    slots = []
    values = {}
    occurrences: list[tuple[int, LeafOccurrence]] = []
    for item in leaves:
        name = names[id(item)]
        found = _occurrences(part, units, item, name)
        spans = tuple(
            sorted(
                {s for _, o in found for s in o.canonical_spans},
                key=lambda s: (s.start, s.end),
            )
        )
        slots.append(item.slot.model_copy(update={"name": name, "occurrences": spans}))
        values[name] = item.value
        occurrences.extend(found)
    occurrences.sort(
        key=lambda pair: (pair[0], pair[1].source_presence != "omitted", pair[1].slot)
    )
    return (
        transformed,
        Recognized(
            LeafSchema(format=2, slots=tuple(slots)),
            values,
            tuple(item for _, item in occurrences),
            base.issues,
            base.low_confidence,
            base.semantics,
        ),
        tuple(item.row for item in leaves),
    )
