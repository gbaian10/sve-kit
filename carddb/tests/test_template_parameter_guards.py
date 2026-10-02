"""Closed schema guards and independent literal/reference value counterexamples."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.snapshot.values import canonical, digest, object_value
from sve_carddb.template_parameters.analysis import prepared
from sve_carddb.template_parameters.models import Range, Schema, Slot, SourceSpan
from sve_carddb.template_parameters.references import References, adopted
from sve_carddb.template_parameters.verification import verify_values
from sve_carddb.template_sources.normalizer import partition

from .test_template_parameters import HASH, candidate
from .translation_fixtures import envelope, term, write

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_inputs import Source
    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.registry.records import RecordData


def checked(model: type[RecordData], data: dict[str, object]) -> None:
    """Keep a single complete validation message, without Pydantic's contextual wrapper."""
    try:
        model.model_validate(data)
    except ValidationError as error:
        failures = error.errors()
        assert len(failures) == 1
        raise ValueError(
            str(failures[0]["msg"]).removeprefix("Value error, ")
        ) from error


@pytest.mark.parametrize(
    ("model", "data", "message"),
    [
        (
            Range,
            {"start": 2, "end": 2},
            "Parameter range must be nonempty and increasing",
        ),
        (
            SourceSpan,
            {
                "role": "body",
                "segments": (Range(start=1, end=3), Range(start=2, end=4)),
                "anchor": None,
            },
            "Source span segments must be sorted and disjoint",
        ),
        (
            Slot,
            {
                "name": "count",
                "type": "uint",
                "occurrences": (Range(start=3, end=4), Range(start=0, end=1)),
                "reference_kind": None,
                "min": 0,
                "max": 1,
            },
            "Slot occurrences must be sorted and disjoint",
        ),
        (
            Slot,
            {
                "name": "layout",
                "type": "literal",
                "occurrences": (Range(start=0, end=1),),
                "reference_kind": None,
                "min": 0,
                "max": 1,
            },
            "Only unsigned slots may declare bounds",
        ),
    ],
)
def test_closed_models_reject_one_specific_invalid_shape(
    model: type[RecordData], data: dict[str, object], message: str
) -> None:
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        checked(model, data)


def test_schema_slot_names_and_first_occurrence_order_are_independently_checked() -> (
    None
):
    result = candidate("試験２枚／３枚")
    assert result.parameter_schema is not None
    slots = result.parameter_schema.slots
    for damaged, message in (
        (
            (slots[0], slots[1].model_copy(update={"name": slots[0].name})),
            "Parameter slot names must be unique",
        ),
        (tuple(reversed(slots)), "Parameter slots must follow their first occurrence"),
    ):
        with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
            checked(Schema, {"slots": damaged})


def test_parameter_occurrence_cannot_extend_past_the_normalized_payload() -> None:
    result = candidate("試験２枚")
    assert result.parameter_schema is not None
    span = Range(start=2, end=99)
    hints = (result.slots[0].model_copy(update={"occurrence": span}),)
    schema = Schema(
        slots=(
            result.parameter_schema.slots[0].model_copy(
                update={"occurrences": (span,)}
            ),
        )
    )
    with pytest.raises(
        ValueError, match=r"\AParameter occurrence must be inside normalized text\Z"
    ):
        verify_values("試験２枚", "試験N枚", schema, hints)


def test_literal_parameter_cannot_hide_a_foreign_sentence() -> None:
    result = candidate(" ")
    assert result.parameter_schema is not None
    hint = result.slots[0].model_copy(
        update={
            "raw_hash": digest(b"Synthetic foreign sentence"),
            "source_segments": (Range(start=0, end=len("Synthetic foreign sentence")),),
        }
    )
    with pytest.raises(
        ValueError,
        match=r"\ALiteral parameter must be exact source layout whitespace\Z",
    ):
        verify_values(
            "Synthetic foreign sentence", "W", result.parameter_schema, (hint,)
        )
    with pytest.raises(
        ValueError,
        match=r"\AFixed layout recipe requires exact nonempty source whitespace\Z",
    ):
        prepared("Synthetic", replace(partition("Synthetic")[0], role="layout"))


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("target", "Reference parameter requires matching adopted concept evidence"),
        ("id", "Reference parameter requires matching adopted concept evidence"),
        ("hash", "Reference parameter requires matching adopted concept evidence"),
        (
            "card",
            "Reference parameter requires an implemented adopted evidence adapter",
        ),
        ("extra", "Reference parameter requires matching adopted concept evidence"),
    ],
)
def test_reference_value_requires_matching_kind_id_and_adopted_evidence(
    change: str, message: str
) -> None:
    text = "『Synthetic』"
    refs = References(card_names={"Synthetic": [("term:name.synthetic", HASH)]})
    result = candidate(text, refs)
    assert result.parameter_schema is not None
    hint = result.slots[0]
    assert hint.target is not None
    target = dict(hint.target)
    schema = result.parameter_schema
    if change == "target":
        target["kind"] = "vocabulary"
    elif change == "id":
        target["id"] = "guessed"
    elif change == "hash":
        target["record_hash"] = "not-approved"
    elif change == "extra":
        target["face_id"] = "f:" + "a" * 32
    else:
        target = {"kind": "card", "id": "c:" + "a" * 32, "record_hash": HASH}
        hint = hint.model_copy(update={"reference_kind": "card"})
        schema = Schema(
            slots=(schema.slots[0].model_copy(update={"reference_kind": "card"}),)
        )
    hint = hint.model_copy(update={"target": target})
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        verify_values(text, "『X』", schema, (hint,))


def test_adopted_exact_authored_concepts_reuse_the_full_glossary_loader(
    tmp_path: Path,
) -> None:
    records = [
        term("name.synthetic", category="card_name"),
        term("ability.synthetic", category="ability"),
    ]
    for record in records:
        object_value(record["data"]).update(
            {
                "source_ref": None,
                "authored_source_ja": "Synthetic",
                "missing_source_reason": "Synthetic authored concept.",
            }
        )
    write(tmp_path, {"translations/glossary/concepts/001.yaml": envelope(records)})

    # These concepts have no raw refs, so no source reader is consulted.
    class UnusedSources:
        def text(self, ref: SourceRef) -> tuple[str, str, Source]:
            assert ref is not None
            pytest.fail("Authored concept must not invent frozen evidence")

    refs = adopted(tmp_path, UnusedSources())
    assert refs.quoted("Synthetic").issues == ()
    assert refs.quoted("Synthetic").target is not None
    assert refs.quoted("Synthetic ").issues == ("missing_card_name_concept",)
    assert len(refs.term_mentions("Synthetic")) == len(records)
    assert "glossary" in refs.pins
    assert canonical(refs.pins)
