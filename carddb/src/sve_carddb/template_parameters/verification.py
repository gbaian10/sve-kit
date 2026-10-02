"""Validate slot values independently from full-field byte coverage and candidate generation."""

import re
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import digest
from sve_carddb.template_parameters.analysis import analyze, prepared, unsigned

if TYPE_CHECKING:
    from sve_carddb.template_parameters.models import Candidate, Hint, Schema, Slot
    from sve_carddb.template_parameters.references import References
    from sve_carddb.template_parameters.spans import Located
    from sve_carddb.template_sources.models import Entry
    from sve_carddb.template_sources.normalizer import Part


def verify_values(
    text: str, normalized: str, schema: Schema, hints: tuple[Hint, ...]
) -> None:
    """Compare declared occurrences, types, exact raw values and repeated-parameter equality."""
    selected: set[int] = set()
    for slot in schema.slots:
        values = []
        for occurrence in slot.occurrences:
            if occurrence.end > len(normalized):
                raise ValueError("Parameter occurrence must be inside normalized text")
            found = [
                (index, h)
                for index, h in enumerate(hints)
                if h.occurrence == occurrence and h.name == slot.name
            ]
            if len(found) != 1:
                raise ValueError(
                    "Parameter schema must cover every classified hint exactly once"
                )
            index, hint = found[0]
            if index in selected:
                raise ValueError(
                    "Parameter schema must cover every classified hint exactly once"
                )
            selected.add(index)
            if (
                hint.issues
                or hint.type != slot.type
                or hint.reference_kind != slot.reference_kind
            ):
                raise ValueError(
                    "Parameter schema requires matching resolved slot types"
                )
            values.append(_occurrence_value(text, normalized, slot, hint))
        if any(value != values[0] for value in values[1:]):
            raise ValueError(
                "Repeated parameter occurrences must have identical values"
            )
    if selected != set(range(len(hints))):
        raise ValueError(
            "Parameter schema must cover every classified hint exactly once"
        )


def _occurrence_value(text: str, normalized: str, slot: Slot, hint: Hint) -> object:
    raw = "".join(text[s.start : s.end] for s in hint.source_segments)
    if (
        any(s.end > len(text) for s in hint.source_segments)
        or digest(raw.encode()) != hint.raw_hash
    ):
        raise ValueError("Parameter raw spans must match exact source spelling hashes")
    occurrence = hint.occurrence
    if (
        digest(normalized[occurrence.start : occurrence.end].encode())
        != hint.normalized_hash
    ):
        raise ValueError("Parameter occurrence must match exact normalized hashes")
    value = _value(raw, slot.type, hint)
    if slot.type == "uint" and (
        slot.min is None
        or slot.max is None
        or hint.value is None
        or not slot.min <= hint.value <= slot.max
    ):
        raise ValueError("Unsigned parameter value must satisfy declared bounds")
    return value


def _value(raw: str, kind: str, hint: Hint) -> object:
    if kind == "uint":
        value = unsigned(raw)
        if value is None or hint.value != value or hint.target is not None:
            raise ValueError("Unsigned parameter must equal its safe decimal raw value")
        return value
    if kind == "literal":
        if (
            not raw.isspace()
            or hint.semantic_role != "layout"
            or hint.target is not None
            or hint.value is not None
        ):
            raise ValueError("Literal parameter must be exact source layout whitespace")
        return raw
    if (
        hint.target is None
        or hint.target.get("kind") != hint.reference_kind
        or hint.value is not None
    ):
        raise ValueError(
            "Reference parameter requires matching adopted concept evidence"
        )
    if hint.reference_kind != "term":
        # No adopted catalog/card-name identity adapter exists at this candidate checkpoint.
        raise ValueError(
            "Reference parameter requires an implemented adopted evidence adapter"
        )
    identifier = hint.target.get("id")
    checksum = hint.target.get("record_hash")
    if (
        set(hint.target) != {"kind", "id", "record_hash"}
        or not isinstance(identifier, str)
        or re.fullmatch(r"term:[a-z][a-z0-9_.-]*", identifier) is None
        or not isinstance(checksum, str)
        or re.fullmatch(r"sha256:[0-9a-f]{64}", checksum) is None
    ):
        raise ValueError(
            "Reference parameter requires matching adopted concept evidence"
        )
    return hint.target


def verify_candidate(
    text: str,
    part: Part,
    entry: Entry,
    located: Located,
    refs: References,
    candidate: Candidate,
) -> None:
    """Reject missed replacements or changed literal provenance, even if a local schema is valid."""
    if candidate.normalized_hash != digest(part.normalized.encode()):
        raise ValueError("Parameter candidate must match the pinned normalized hash")
    if candidate.parameter_schema is not None:
        template_part, _ = prepared(text, part)
        verify_values(
            text, template_part.normalized, candidate.parameter_schema, candidate.slots
        )
    if candidate != analyze(text, part, entry, located, refs):
        raise ValueError(
            "Parameter candidate must replay exact spans roles and semantic evidence"
        )
