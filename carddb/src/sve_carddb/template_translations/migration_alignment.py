"""Conservative target alignment for anonymous private drafts, without positional guesses."""

import re
from collections import Counter
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameters.models import Range
from sve_carddb.template_translations.preparation import Alignment, TargetSlot, convert

if TYPE_CHECKING:
    from sve_carddb.template_parameters.models import Schema, Slot
    from sve_carddb.template_translations.preparation import Draft, Prepared

MARKER = re.compile(r"(?<![A-Za-z0-9_])[NX](?![A-Za-z0-9_])")


def align(
    draft: Draft,
    normalized: str,
    schema: Schema,
    labels: dict[str, str],
    *,
    template_id: str,
) -> Prepared:
    """One anonymous marker may identify one slot, never several slots by their order."""
    markers: dict[str, str] = {}
    for slot in schema.slots:
        values = {normalized[r.start : r.end] for r in slot.occurrences}
        if len(values) != 1:
            raise ValueError("draft_slot_source_values_differ")
        source = next(iter(values))
        if slot.type == "uint" and source == "N":
            marker = "N"
        elif slot.type == "reference" and source == "X":
            marker = "X"
        elif slot.type == "reference" and slot.name in labels:
            marker = labels[slot.name]
        else:
            raise ValueError("draft_slot_target_not_identified")
        if not marker or marker in markers:
            raise ValueError("draft_anonymous_slot_ambiguous")
        markers[marker] = slot.name
    for slot in schema.slots:
        marker = next(value for value, name in markers.items() if name == slot.name)
        if marker in {"N", "X"}:
            _source_marker(normalized, slot, marker)
    target = _target_slots(draft, schema, markers)
    return convert(
        draft,
        schema,
        Alignment(
            draft_text_hash=digest(draft.zh.encode()),
            schema_hash=digest(canonical(schema.model_dump(mode="json"))),
            slots=target,
        ),
        template_id=template_id,
    )


def _source_marker(normalized: str, slot: Slot, marker: str) -> None:
    expected = {(r.start, r.end) for r in slot.occurrences}
    actual = {
        (m.start(), m.end()) for m in MARKER.finditer(normalized) if m.group() == marker
    }
    if actual != expected:
        raise ValueError("draft_literal_marker_ambiguous")


def _target_slots(
    draft: Draft, schema: Schema, markers: dict[str, str]
) -> tuple[TargetSlot, ...]:
    declared = {slot.name: len(slot.occurrences) for slot in schema.slots}
    result = []
    covered_markers = set()
    for marker, name in markers.items():
        pattern = (
            re.compile(r"(?<![A-Za-z0-9_])" + marker + r"(?![A-Za-z0-9_])")
            if marker in {"N", "X"}
            else re.compile(re.escape(marker))
        )
        matches = tuple(pattern.finditer(draft.zh))
        if len(matches) != declared[name]:
            raise ValueError("draft_target_occurrence_count_differs")
        for match in matches:
            result.append(
                TargetSlot(
                    name=name,
                    span=Range(start=match.start(), end=match.end()),
                    raw_hash=digest(match.group().encode()),
                )
            )
            covered_markers.add((match.start(), match.end()))
    if any(
        (m.start(), m.end()) not in covered_markers for m in MARKER.finditer(draft.zh)
    ):
        raise ValueError("draft_unbound_target_marker")
    result.sort(key=lambda slot: slot.span.start)
    if Counter(slot.name for slot in result) != Counter(declared):
        raise ValueError("draft_target_schema_coverage_differs")
    return tuple(result)
