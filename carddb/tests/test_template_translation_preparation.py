"""Synthetic finite-language and exact-coordinate checks, without official text or adoption claims."""

import re

import pytest
from pydantic import ValidationError

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameters.models import Range, Schema, Slot
from sve_carddb.template_translations.preparation import (
    Alignment,
    Draft,
    TargetSlot,
    convert,
)
from sve_carddb.template_translations.text import Literal, Parameter, escape, parse


def schema(*names: str) -> Schema:
    """Use distinct synthetic positions; prose is not a source of inferred roles."""
    return Schema(
        slots=tuple(
            Slot(
                name=name,
                type="uint",
                occurrences=(Range(start=i * 2, end=i * 2 + 1),),
                reference_kind=None,
                min=0,
                max=9007199254740991,
            )
            for i, name in enumerate(names)
        )
    )


def draft(text: str = "合成 N 與 N，literal N {標記}\\") -> Draft:
    normalized = "N合成N，literal N"
    return Draft(
        template="T" + digest(normalized.encode())[7:17],
        normalized=normalized,
        zh=text,
        confidence="low",
        note="Synthetic uncertain wording",
    )


def alignment(value: Draft, shape: Schema) -> Alignment:
    return Alignment(
        draft_text_hash=digest(value.zh.encode()),
        schema_hash=digest(canonical(shape.model_dump(mode="json"))),
        slots=tuple(
            TargetSlot(
                name=name, span=Range(start=start, end=start + 1), raw_hash=digest(b"N")
            )
            for name, start in (("slot_1", 3), ("slot_0", 7))
        ),
    )


def test_explicit_target_order_preserves_literal_letters_and_machine_uncertainty() -> (
    None
):
    value, shape = draft(), schema("slot_0", "slot_1")
    result = convert(value, shape, alignment(value, shape), template_id=value.template)
    assert result.text == "合成 {{slot_1}} 與 {{slot_0}}，literal N \\{標記\\}\\\\"
    assert result.text_hash == digest(result.text.encode())
    assert result.state == "candidate_only"
    assert result.origin == "machine"
    assert result.confidence == "low"
    assert not hasattr(result, "decision_id")
    assert not hasattr(result, "model_review")


def test_zero_slot_draft_is_literal_including_n_x_and_braces() -> None:
    value, shape = draft("字面 N/X {合成}\\"), Schema(slots=())
    mapping = Alignment(
        draft_text_hash=digest(value.zh.encode()),
        schema_hash=digest(canonical(shape.model_dump(mode="json"))),
        slots=(),
    )
    result = convert(value, shape, mapping, template_id=value.template)
    assert result.text == "字面 N/X \\{合成\\}\\\\"
    assert parse(result.text, shape) == (Literal(value.zh),)


def test_reference_quotes_and_signs_stay_outside_target_slots() -> None:
    shape = Schema(
        slots=(
            Slot(
                name="name",
                type="reference",
                occurrences=(Range(start=1, end=2),),
                reference_kind="term",
                min=None,
                max=None,
            ),
        )
    )
    value = draft("『X』，+N")
    mapping = Alignment(
        draft_text_hash=digest(value.zh.encode()),
        schema_hash=digest(canonical(shape.model_dump(mode="json"))),
        slots=(
            TargetSlot(name="name", span=Range(start=1, end=2), raw_hash=digest(b"X")),
        ),
    )
    result = convert(value, shape, mapping, template_id=value.template)
    assert result.text == "『{{name}}』，+N"


@pytest.mark.parametrize(
    "value", ["", "{", "}", "\\", "{{literal}}", "中文Ⓢ１２", "\\{x}\\"]
)
def test_literal_escaping_roundtrips(value: str) -> None:
    assert (
        "".join(
            p.text
            for p in parse(escape(value), Schema(slots=()))
            if isinstance(p, Literal)
        )
        == value
    )


def test_repeated_slots_and_target_code_point_coordinates() -> None:
    shape = Schema(
        slots=(
            schema("amount")
            .slots[0]
            .model_copy(
                update={"occurrences": (Range(start=0, end=1), Range(start=2, end=3))}
            ),
        )
    )
    value = draft("🧪NⓈＮ")
    mapping = Alignment(
        draft_text_hash=digest(value.zh.encode()),
        schema_hash=digest(canonical(shape.model_dump(mode="json"))),
        slots=(
            TargetSlot(
                name="amount", span=Range(start=1, end=2), raw_hash=digest(b"N")
            ),
            TargetSlot(
                name="amount",
                span=Range(start=3, end=4),
                raw_hash=digest("Ｎ".encode()),
            ),
        ),
    )
    result = convert(value, shape, mapping, template_id="Tsynthetic")
    assert result.text == "🧪{{amount}}Ⓢ{{amount}}"
    assert parse(result.text, shape) == (
        Literal("🧪"),
        Parameter("amount"),
        Literal("Ⓢ"),
        Parameter("amount"),
    )


@pytest.mark.parametrize(
    ("text", "names", "message"),
    [
        ("{{x", ("x",), "Template parameter must have closing double braces"),
        ("{{ x }}", ("x",), "Template parameter must be a plain ASCII slot name"),
        ("{{x + 1}}", ("x",), "Template parameter must be a plain ASCII slot name"),
        (
            "{{__import__('os')}}",
            ("x",),
            "Template parameter must be a plain ASCII slot name",
        ),
        ("{{unknown}}", ("x",), "Template parameter is not declared by its schema"),
        ("{x}", (), "Template literal braces must be escaped"),
        ("}", (), "Template literal braces must be escaped"),
        ("\\n", (), "Template literal escape must precede a brace or backslash"),
        ("end\\", (), "Template literal escape must precede a brace or backslash"),
        ("plain", ("x",), "Template text must use every declared parameter"),
        ("\\{\\{x\\}\\}", ("x",), "Template text must use every declared parameter"),
        ("{{x}}}", ("x",), "Template literal braces must be escaped"),
    ],
)
def test_text_rejections(text: str, names: tuple[str, ...], message: str) -> None:
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        parse(text, schema(*names))


@pytest.mark.parametrize(
    ("change", "message"),
    [
        (
            {"draft_text_hash": digest(b"other")},
            "Translation alignment must pin the exact draft text",
        ),
        (
            {"schema_hash": digest(b"other")},
            "Translation alignment must pin the exact parameter schema",
        ),
        (
            {"slots": ()},
            "Translation alignment must cover each schema occurrence exactly once",
        ),
    ],
)
def test_alignment_pin_and_missing_members_rejected(
    change: dict[str, object], message: str
) -> None:
    value, shape = draft(), schema("slot_0", "slot_1")
    mapping = alignment(value, shape).model_copy(update=change)
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        convert(value, shape, mapping, template_id=value.template)


def test_alignment_wrong_duplicate_name_cannot_replace_missing_slot() -> None:
    value, shape = draft(), schema("slot_0", "slot_1")
    mapping = alignment(value, shape)
    wrong = mapping.model_copy(
        update={
            "slots": (
                mapping.slots[0],
                mapping.slots[1].model_copy(update={"name": "slot_1"}),
            )
        }
    )
    with pytest.raises(
        ValueError,
        match=r"^Translation alignment must cover each schema occurrence exactly once$",
    ):
        convert(value, shape, wrong, template_id=value.template)


@pytest.mark.parametrize(
    ("span", "checksum", "message"),
    [
        (
            Range(start=7, end=999),
            digest(b"N"),
            "Translation target span must be inside the exact draft text",
        ),
        (
            Range(start=7, end=8),
            digest(b"x"),
            "Translation target span must match its exact raw hash",
        ),
    ],
)
def test_alignment_bad_span_or_raw_hash_rejected(
    span: Range, checksum: str, message: str
) -> None:
    value, shape = draft(), schema("slot_0", "slot_1")
    mapping = alignment(value, shape)
    wrong = mapping.model_copy(
        update={
            "slots": (
                mapping.slots[0],
                mapping.slots[1].model_copy(
                    update={"span": span, "raw_hash": checksum}
                ),
            )
        }
    )
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        convert(value, shape, wrong, template_id=value.template)


def test_alignment_overlap_rejected_at_closed_model_boundary() -> None:
    value, shape = draft(), schema("slot_0", "slot_1")
    mapping = alignment(value, shape).model_dump(mode="json")
    mapping["slots"][1]["span"] = {"start": 3, "end": 4}
    with pytest.raises(
        ValidationError,
        match="Translation target slot spans must be sorted and disjoint",
    ):
        Alignment.model_validate_json(canonical(mapping))


def test_legacy_fingerprint_and_extra_authority_are_rejected() -> None:
    value = draft().model_dump(mode="json")
    value["template"] = "T0000000000"
    with pytest.raises(
        ValidationError,
        match="Translation draft must reproduce its legacy normalized fingerprint",
    ):
        Draft.model_validate_json(canonical(value))
    value = draft().model_dump(mode="json")
    value["decision_id"] = "synthetic purported adoption"
    with pytest.raises(ValidationError, match="Extra inputs are not permitted"):
        Draft.model_validate_json(canonical(value))
