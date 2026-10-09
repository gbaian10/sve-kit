"""Closed schema guards and independent literal/reference value counterexamples."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.contracts.template_parameters import Range, Schema, Slot, SourceSpan
from sve_carddb.core.json import canonical, object_value
from sve_carddb.core.provenance import ArchivePin, Source
from sve_carddb.domains.translations.inputs import load_glossary
from sve_carddb.domains.translations.parameters.adopted_references import adopted
from sve_carddb.domains.translations.parameters.analysis import (
    header_positions,
    prepared,
)
from sve_carddb.domains.translations.parameters.references import References
from sve_carddb.domains.translations.parameters.verification import verify_values
from sve_carddb.domains.translations.source_inventory.normalizer import partition

from ...support.translation_fixtures import envelope, term, write
from .test_template_parameters import HASH, candidate, numeric_fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.models import RecordData
    from sve_carddb.domains.catalog.adoption_models import SourceRef


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


@pytest.mark.parametrize("value", [True, 1.0, "1", None])
def test_parameter_format_is_exact_integer_not_a_coercible_literal(
    value: object,
) -> None:
    with pytest.raises(
        ValueError, match=r"\AParameter schema format must be integer one\Z"
    ):
        checked(Schema, {"format": value, "slots": ()})
    assert Schema(format=1, slots=()).format == 1


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
    shape, _ = numeric_fixture("試験２枚／３枚")
    slots = shape.slots
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
    shape, original_hints = numeric_fixture("試験２枚")
    span = Range(start=2, end=99)
    hints = (original_hints[0].model_copy(update={"occurrence": span}),)
    schema = Schema(slots=(shape.slots[0].model_copy(update={"occurrences": (span,)}),))
    with pytest.raises(
        ValueError, match=r"\AParameter occurrence must be inside normalized text\Z"
    ):
        verify_values("試験２枚", "試験N枚", schema, hints)


def test_literal_parameter_cannot_hide_a_foreign_sentence() -> None:
    result = candidate(" ")
    assert result.parameter_schema is not None
    hint = result.slots[0].model_copy(
        update={
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
    refs = References(card_names={"Synthetic": ["term:name.synthetic"]})
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
    elif change == "extra":
        target["face_id"] = "f:" + "a" * 32
    else:
        target = {"kind": "card", "id": "c:" + "a" * 32}
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

    refs = adopted(load_glossary(tmp_path), UnusedSources())
    assert refs.quoted("Synthetic").issues == ()
    assert refs.quoted("Synthetic").target is not None
    assert refs.quoted("Synthetic ").issues == ("missing_card_name_concept",)
    assert len(refs.term_mentions("Synthetic")) == len(records)
    assert "exact_concepts_hash" in refs.pins
    assert canonical(refs.pins)


def test_header_candidate_must_match_the_entire_legacy_grammar() -> None:
    with pytest.raises(
        ValueError,
        match=r"\AToken header candidate must match the complete legacy header grammar\Z",
    ):
        header_positions("Synthetic invalid header")


@pytest.mark.parametrize("damage", ["unmatched", "duplicate_hint", "omitted", "reused"])
def test_schema_covers_each_hint_once_at_each_independent_guard(damage: str) -> None:
    shape, hints = numeric_fixture("試験２枚／２枚")
    if damage == "unmatched":
        shape = Schema(slots=(shape.slots[0].model_copy(update={"name": "other"}),))
    elif damage == "duplicate_hint":
        hints = (hints[0], hints[0], hints[1])
    elif damage == "omitted":
        shape = Schema(slots=shape.slots[:1])
    else:
        # Validated Schema rejects overlap first; simulate a broken internal producer.
        shape = Schema.model_construct(
            slots=(shape.slots[0], shape.slots[0], shape.slots[1])
        )
    with pytest.raises(
        ValueError,
        match=r"\AParameter schema must cover every classified hint exactly once\Z",
    ):
        verify_values("試験２枚／２枚", "試験N枚／N枚", shape, hints)


@pytest.mark.parametrize("lang", ["en", "ja"])
def test_adopted_concept_requires_japanese_and_nonempty_exact_frozen_text(
    tmp_path: Path, lang: str
) -> None:
    write(tmp_path, {"translations/glossary/concepts/001.yaml": envelope([term()])})

    class Sources:
        def text(self, ref: SourceRef) -> tuple[str, str, Source]:
            source = Source(
                id=ref.source_version_id,
                sha256=HASH,
                raw_locator="synthetic/raw",
                parser_version=ref.parser,
                archive=ArchivePin(
                    store_id="test-store",
                    batch_id=ref.batch_id,
                    descriptor_sha256=HASH,
                    first_receipt_id=HASH,
                ),
                url="https://synthetic.invalid/card",
                fetched_at="2026-10-02T00:00:00Z",
            )
            return lang, "Synthetic" if lang == "en" else "", source

    message = (
        "Parameter concepts require exact Japanese source names"
        if lang == "en"
        else "Parameter concept source must be nonempty exact text"
    )
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        adopted(load_glossary(tmp_path), Sources())
