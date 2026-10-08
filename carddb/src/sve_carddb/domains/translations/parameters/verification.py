"""Validate slot values independently from full-field byte coverage and candidate generation."""

import re
from typing import TYPE_CHECKING

from sve_carddb.domains.translations.parameters.analysis import unsigned

if TYPE_CHECKING:
    from sve_carddb.contracts.template_parameters import Schema, Slot
    from sve_carddb.domains.translations.parameters.models import Hint


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
            values.append(_occurrence_value(text, slot, hint))
        if any(value != values[0] for value in values[1:]):
            raise ValueError(
                "Repeated parameter occurrences must have identical values"
            )
    if selected != set(range(len(hints))):
        raise ValueError(
            "Parameter schema must cover every classified hint exactly once"
        )


def _occurrence_value(text: str, slot: Slot, hint: Hint) -> object:
    raw = "".join(text[s.start : s.end] for s in hint.source_segments)
    if not hint.source_segments or any(s.end > len(text) for s in hint.source_segments):
        raise ValueError("Parameter source spans must be inside source text")
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
    if (hint.reference_kind == "vocabulary" and _current_vocabulary(hint)) or (
        hint.reference_kind == "term" and _card_name_fallback(raw, hint)
    ):
        return hint.target
    if hint.reference_kind != "term":
        raise ValueError(
            "Reference parameter requires an implemented adopted evidence adapter"
        )
    identifier = hint.target.get("id")
    if (
        set(hint.target) != {"kind", "id"}
        or not isinstance(identifier, str)
        or re.fullmatch(r"term:[a-z][a-z0-9_.-]*", identifier) is None
    ):
        raise ValueError(
            "Reference parameter requires matching adopted concept evidence"
        )
    return hint.target


def _card_name_fallback(raw: str, hint: Hint) -> bool:
    """A recognized name without a concept may only carry its own exact source spelling."""
    target = hint.target
    if target is None or set(target) != {"kind", "card_name"}:
        return False
    if (
        hint.semantic_role not in {"quoted_reference", "card_name"}
        or target["card_name"] != raw
    ):
        raise ValueError("Card-name fallback must keep its exact quoted spelling")
    return True


def _current_vocabulary(hint: Hint) -> bool:
    target = hint.target
    if target is None or set(target) != {
        "kind",
        "vocabulary_kind",
        "vocabulary_code",
        "special_kinds",
    }:
        return False
    kind, code = target["vocabulary_kind"], target["vocabulary_code"]
    if not isinstance(kind, str) or not isinstance(code, str):
        return False
    return (
        kind in {"class", "type"}
        and kind == hint.semantic_role
        and re.fullmatch(r"[a-z][a-z0-9_-]*", code) is not None
        and target["special_kinds"] == []
    )
