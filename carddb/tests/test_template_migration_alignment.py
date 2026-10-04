"""Anonymous drafts cannot obtain parameter identities from translation word order."""

import re

import pytest
from pydantic import JsonValue

from sve_carddb.snapshot.values import digest
from sve_carddb.template_parameters.models import Range, Schema, Slot
from sve_carddb.template_translations.migration_alignment import align
from sve_carddb.template_translations.migration_drafts import effect_drafts
from sve_carddb.template_translations.preparation import Draft


def draft(source: str, text: str) -> Draft:
    return Draft(
        template="T" + digest(source.encode())[7:17],
        normalized=source,
        zh=text,
        confidence="high",
        note="",
    )


def number(name: str, start: int) -> Slot:
    return Slot(
        name=name,
        type="uint",
        occurrences=(Range(start=start, end=start + 1),),
        reference_kind=None,
        min=0,
        max=9007199254740991,
    )


def test_one_marker_maps_to_its_unique_semantic_slot() -> None:
    source = "Test N times"
    schema = Schema(slots=(number("count", 5),))
    result = align(draft(source, "測試 N 次。"), source, schema, {}, template_id="test")
    assert result.text == "測試 {{count}} 次。"
    assert result.origin == "machine"


def test_distinct_numeric_roles_cannot_use_target_order() -> None:
    source = "N then N"
    schema = Schema(slots=(number("attack", 0), number("health", 7)))
    with pytest.raises(ValueError, match=r"^draft_anonymous_slot_ambiguous$"):
        align(draft(source, "N 然後 N"), source, schema, {}, template_id="test")


def test_literal_marker_is_not_a_parameter() -> None:
    source = "N Test N"
    schema = Schema(slots=(number("count", 7),))
    with pytest.raises(ValueError, match=r"^draft_literal_marker_ambiguous$"):
        align(draft(source, "N 測試 N"), source, schema, {}, template_id="test")


def test_zero_slots_escape_punctuation_without_turning_it_into_parameters() -> None:
    result = align(
        draft("Test", "測試 {字}\\。"), "Test", Schema(slots=()), {}, template_id="test"
    )
    assert result.text == "測試 \\{字\\}\\\\。"


@pytest.mark.parametrize("text", ["N 與 N", "沒有數字", "能力_N"])
def test_target_must_cover_identified_marker_exactly(text: str) -> None:
    schema = Schema(slots=(number("count", 5),))
    with pytest.raises(ValueError, match=r"^draft_target_occurrence_count_differs$"):
        align(draft("Test N", text), "Test N", schema, {}, template_id="test")


def test_reference_requires_an_explicit_target_label() -> None:
    schema = Schema(
        slots=(
            Slot(
                name="term",
                type="reference",
                occurrences=(Range(start=0, end=4),),
                reference_kind="term",
                min=None,
                max=None,
            ),
        )
    )
    source = "Test term"
    with pytest.raises(ValueError, match=r"^draft_slot_target_not_identified$"):
        align(draft(source, "名詞"), source, schema, {}, template_id="test")
    result = align(
        draft(source, "名詞"), source, schema, {"term": "名詞"}, template_id="test"
    )
    assert result.text == "{{term}}"


def test_low_confidence_is_union_of_original_flag_and_unsure_review() -> None:
    from sve_carddb.snapshot.values import canonical  # ruff: ignore[import-outside-top-level] -- compact synthetic input

    a, b, c = (draft(name, "測試") for name in ("Test A", "Test B", "Test C"))
    rows = (a.model_copy(update={"confidence": "low"}), b, c)
    raw = b"\n".join(canonical(row.model_dump(mode="json")) for row in rows)
    reviews = b"\n".join(
        canonical({"template": row.template, "verdict": verdict})
        for row, verdict in zip(rows, ("ok", "unsure", "suggest"), strict=True)
    )
    loaded = effect_drafts(raw, reviews)
    by_id = {item.draft.template: item for item in loaded}
    assert by_id[a.template].low_confidence
    assert by_id[b.template].low_confidence
    assert not by_id[c.template].low_confidence
    with pytest.raises(
        ValueError,
        match="^"
        + re.escape("Template draft review coverage differs from drafts")
        + "$",
    ):
        effect_drafts(raw, reviews.splitlines()[0])


def test_flavor_draft_preserves_exact_paragraphs_and_quality() -> None:
    from sve_carddb.snapshot.values import canonical  # ruff: ignore[import-outside-top-level] -- synthetic private input
    from sve_carddb.template_translations.migration_drafts import flavor_drafts  # ruff: ignore[import-outside-top-level] -- exact flavor reader

    source = " Synthetic２\r\n Test "
    value: dict[str, JsonValue] = {
        "flavor_id": digest(source.encode())[7:23],
        "source_text": source,
        "zh_hant": "自撰譯文",
        "confidence": "low",
    }
    loaded = flavor_drafts(canonical(value))[0]
    assert loaded.source_text == source
    assert loaded.low_confidence
    assert loaded.source_hash == digest(source.encode())
    with pytest.raises(
        ValueError, match=r"^Duplicate or mismatched flavor draft fingerprint$"
    ):
        flavor_drafts(canonical({**value, "flavor_id": "0" * 16}))
    with pytest.raises(
        ValueError, match=r"^Duplicate or mismatched flavor draft fingerprint$"
    ):
        flavor_drafts(canonical(value) + b"\n" + canonical(value))
