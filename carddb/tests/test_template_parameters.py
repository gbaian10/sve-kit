"""Invented independent uint, layout, concept, vocabulary and legacy-fork counterexamples."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.snapshot.values import digest, object_value
from sve_carddb.template_parameters.analysis import (
    NUMERIC_RULE_PENDING,
    SAFE_INTEGER,
    analyze,
    schema,
    unsigned,
)
from sve_carddb.template_parameters.inventory import Candidates, lineage, summary
from sve_carddb.template_parameters.models import Range, Schema, Slot, SourceSpan
from sve_carddb.template_parameters.references import References
from sve_carddb.template_parameters.spans import locate
from sve_carddb.template_parameters.verification import verify_candidate, verify_values
from sve_carddb.template_sources.inventory import entry
from sve_carddb.template_sources.normalizer import VERSION, partition
from sve_carddb.template_sources.pins import PARSER
from sve_carddb.text_observations.vocabulary import Binding, Vocabulary

if TYPE_CHECKING:
    from sve_carddb.template_parameters.models import Candidate, Hint

HASH = "sha256:" + "a" * 64


def candidate(
    text: str, refs: References | None = None, *, section: int | None = None
) -> Candidate:
    parts = partition(text, section=section)
    position = locate(text, parts)[0]
    part = parts[0]
    ref = SourceRef(
        store_id="synthetic",
        batch_id=HASH,
        source_version_id="src:v1:" + "b" * 64,
        parser=PARSER,
        locator="/faces/0/text" if section is None else f"/faces/0/sections/{section}",
        text_hash=digest(text.encode()),
    )
    item = entry(ref, part, VERSION)
    evidence = refs or References()
    result = analyze(text, part, item, position, evidence)
    verify_candidate(text, part, item, position, evidence, result)
    return result


def numeric_fixture(text: str) -> tuple[Schema, tuple[Hint, ...]]:
    """Supply synthetic resolved hints only to isolate downstream value-verifier guards."""
    hints = tuple(h.model_copy(update={"issues": ()}) for h in candidate(text).slots)
    assert all(h.issues == (NUMERIC_RULE_PENDING,) for h in candidate(text).slots)
    shape = schema(hints)
    assert shape is not None
    return shape, hints


def test_numeric_slot_has_safe_value_raw_spelling_and_only_its_true_occurrence() -> (
    None
):
    result = candidate("N試験２枚 X")
    assert result.parameter_schema is None
    shape, _ = numeric_fixture("N試験２枚 X")
    slot = shape.slots[0]
    assert (slot.type, slot.min, slot.max) == ("uint", 0, SAFE_INTEGER)
    assert slot.occurrences == (Range(start=3, end=4),)
    assert result.slots[0].value == 2
    assert result.slots[0].raw_hash == digest("２".encode())
    assert result.slots[0].normalized_hash == digest(b"N")
    assert result.literal_trace[0].occurrence == Range(start=0, end=3)
    assert result.literal_trace[-1].occurrence == Range(start=4, end=7)
    assert result.slots[0].numeric_rule == "suffix_unit_cards"
    assert result.issues == (NUMERIC_RULE_PENDING,)
    assert result.payload_hash is None
    assert candidate("N試験３枚 X").issues == result.issues
    assert unsigned("0002") == 2


@pytest.mark.parametrize(
    ("text", "reason"),
    [
        ("-２枚", "signed_numeric_requires_review"),
        ("SYN２枚", "numeric_identifier_requires_review"),
        ("試験２", "numeric_role_requires_review"),
        ("②枚", "invalid_safe_unsigned_decimal"),
        ("９００７１９９２５４７４０９９２枚", "invalid_safe_unsigned_decimal"),
    ],
)
def test_unproved_or_unsafe_number_never_becomes_a_complete_schema(
    text: str, reason: str
) -> None:
    result = candidate(text)
    assert result.parameter_schema is None
    assert reason in result.issues
    assert result.payload_hash is None


def test_quoted_exact_adopted_concept_is_term_and_delimiters_remain_literal() -> None:
    refs = References(card_names={"Synthetic ２": [("term:name.synthetic", HASH)]})
    result = candidate("試験『Synthetic ２』", refs)
    assert result.parameter_schema is not None
    assert result.parameter_schema.slots[0].reference_kind == "term"
    assert result.parameter_schema.slots[0].occurrences == (Range(start=3, end=4),)
    assert result.slots[0].raw_hash == digest("Synthetic ２".encode())
    assert result.literal_trace[0].occurrence == Range(start=0, end=3)
    assert result.literal_trace[1].occurrence == Range(start=4, end=5)
    assert result.slots[0].target == {
        "kind": "term",
        "id": "term:name.synthetic",
        "record_hash": HASH,
    }


@pytest.mark.parametrize(
    "name", ["Synthetic 2", "Synthetic ２ ", "Ｓynthetic ２", "synthetic ２"]
)
def test_reference_lookup_does_not_trim_casefold_nfkc_or_guess_card_ids(
    name: str,
) -> None:
    refs = References(card_names={"Synthetic ２": [("term:name.synthetic", HASH)]})
    result = candidate("試験『" + name + "』", refs)
    assert result.parameter_schema is None
    assert result.slots[0].target is None
    assert result.issues == ("missing_card_name_concept",)


def test_ambiguous_card_name_and_unclassified_term_do_not_choose_an_arbitrary_target() -> (
    None
):
    refs = References(
        card_names={"Synthetic": [("term:name.one", HASH), ("term:name.two", HASH)]},
        terms={"Synthetic": [("term:rule.synthetic", "rule_term", HASH)]},
    )
    assert candidate("『Synthetic』", refs).issues == ("ambiguous_card_name_concept",)
    refs.card_names.clear()
    assert candidate("『Synthetic』", refs).issues == ("missing_card_name_concept",)
    assert (
        refs.term_mentions("Synthetic")[0]["reason"]
        == "term_mention_requires_semantic_role_review"
    )


def test_braced_term_is_a_slot_candidate_but_ordinary_mentions_are_diagnostics() -> (
    None
):
    refs = References(
        terms={"Synthetic": [("term:ability.synthetic", "ability", HASH)]}
    )
    result = candidate("試験{Synthetic}", refs)
    assert result.slots[0].semantic_role == "term"
    assert result.slots[0].target == {
        "kind": "term",
        "id": "term:ability.synthetic",
        "record_hash": HASH,
    }
    assert result.parameter_schema is None
    assert result.issues == ("term_role_requires_review",)
    assert candidate("試験Synthetic", refs).slots == ()


def test_header_grammar_identifies_optional_numeric_and_class_type_roles_without_adopting_codes() -> (
    None
):
    vocabulary = Vocabulary(
        bindings=(
            Binding(region="jp", kind="class", raw="Synthetic", code="synthetic"),
            Binding(region="jp", kind="type", raw="フォロワー", code="follower"),
        )
    )
    refs = References(vocabulary=vocabulary)
    result = candidate(
        "『Name』{Synthetic}フォロワー{コスト２}{攻撃力}１/{体力}３", refs, section=0
    )
    assert [h.semantic_role for h in result.slots] == [
        "card_name",
        "class",
        "type",
        "cost",
        "attack",
        "health",
    ]
    assert [h.value for h in result.slots[-3:]] == [2, 1, 3]
    assert result.parameter_schema is None
    assert result.slots[1].target is not None
    assert result.slots[1].target["vocabulary_code"] == "synthetic"
    assert result.slots[1].issues == ("vocabulary_not_adopted",)
    assert [
        h.semantic_role
        for h in candidate("『Name』{Synthetic}クレスト", refs, section=0).slots
    ] == ["card_name", "class", "type"]


def test_composite_vocabulary_retains_special_flags_and_requires_separate_slots() -> (
    None
):
    refs = References(
        vocabulary=Vocabulary(
            bindings=(
                Binding(
                    region="jp",
                    kind="type",
                    raw="Composite",
                    code="follower",
                    special_kinds=("evolve",),
                ),
                Binding(
                    region="jp", kind="special_kind", raw="Synthetic", code="evolve"
                ),
            )
        )
    )
    assert refs.vocabulary is not None
    refs.vocabulary.verify()
    result = candidate("試験{Composite}", refs)
    assert result.slots[0].target is not None
    assert result.slots[0].target["special_kinds"] == ["evolve"]
    assert result.parameter_schema is None
    assert "composite_vocabulary_requires_separate_slots" in result.issues
    assert candidate("試験Composite", refs).slots == ()


def test_header_traits_use_the_existing_separator_exceptions_and_exact_adopted_category() -> (
    None
):
    refs = References(
        terms={
            "〈Synthetic・Composite〉": [("term:trait.synthetic", "trait", HASH)],
            "Other": [("term:trait.other", "trait", HASH)],
        },
        vocabulary=Vocabulary(
            bindings=(
                Binding(region="jp", kind="type", raw="フォロワー", code="follower"),
            )
        ),
    )
    result = candidate(
        "『Name』{Synthetic}〈Synthetic・Composite〉・Other・フォロワー",
        refs,
        section=0,
    )
    assert [h.semantic_role for h in result.slots] == [
        "card_name",
        "class",
        "trait",
        "trait",
        "type",
    ]
    assert [
        h.target["id"]
        for h in result.slots
        if h.semantic_role == "trait" and h.target is not None
    ] == ["term:trait.synthetic", "term:trait.other"]
    assert result.slots[-1].target is not None
    assert result.slots[-1].target["vocabulary_code"] == "follower"
    assert all(not h.issues for h in result.slots if h.semantic_role == "trait")
    refs.terms["Other"] = [("term:ability.other", "ability", HASH)]
    assert (
        "unknown_or_ambiguous_header_trait"
        in candidate("『Name』{Synthetic}Other・フォロワー", refs, section=0).issues
    )
    assert (
        "unrecognized_header_trait_layout"
        in candidate("『Name』{Synthetic}Otherフォロワー", refs, section=0).issues
    )
    assert (
        "unrecognized_header_trait_layout"
        in candidate("『Name』{Synthetic}Other・・フォロワー", refs, section=0).issues
    )
    assert (
        "header_empty_numeric_requires_review"
        in candidate("『Name』{Synthetic}フォロワー{コスト}", refs, section=0).issues
    )


def test_layout_is_exact_whitespace_literal_and_reminders_are_not_approved_rules() -> (
    None
):
    result = candidate(" \r\n")
    assert result.parameter_schema is not None
    assert result.slots[0].type == "literal"
    assert result.slots[0].raw_hash == digest(b" \r")
    assert result.slots[0].source_segments == (Range(start=0, end=2),)
    assert result.literal_trace == ()
    assert result.payload_hash == candidate("\t ").payload_hash
    assert result.normalized_hash != candidate("\t ").normalized_hash
    assert result.template_normalized_hash == digest(b"W")
    reminder = candidate("（『Synthetic 2』３枚）")
    assert [h.semantic_role for h in reminder.slots] == ["quoted_reference", "numeric"]
    assert reminder.slots[-1].value == 3
    assert "legacy_parenthesis_classification_requires_review" in reminder.issues


def test_literal_n_and_numeric_n_force_fork_without_fake_parent_payload() -> None:
    ordinary = candidate("試験N枚")
    numeric = candidate("試験２枚")
    assert ordinary.legacy_id == numeric.legacy_id
    parents = lineage(Candidates(entries=[ordinary, numeric]))
    assert parents[0]["requires_provenance_split"] is True
    assert len(object_value(parents[0]["mechanical_variants"])) == 2
    assert parents[0]["supersedes_id"] is None
    assert (
        summary(Candidates(entries=[ordinary, numeric]))["legacy_provenance_forks"] == 1
    )
    damaged = numeric.model_copy(update={"normalized_hash": HASH})
    with pytest.raises(
        ValueError,
        match=r"\ALegacy fingerprint collision must stop parameter candidate grouping\Z",
    ):
        lineage(Candidates(entries=[ordinary, damaged]))


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("value", "Unsigned parameter must equal its safe decimal raw value"),
        ("raw", "Parameter raw spans must match exact source spelling hashes"),
        ("normalized", "Parameter occurrence must match exact normalized hashes"),
        ("missing", "Parameter schema must cover every classified hint exactly once"),
        ("type", "Parameter schema requires matching resolved slot types"),
        ("bounds", "Unsigned parameter value must satisfy declared bounds"),
    ],
)
def test_slot_verifier_rejects_single_damage_with_one_complete_message(
    change: str, message: str
) -> None:
    schema, hints = numeric_fixture("試験２枚")
    updates: dict[str, object] = (
        {"value": 8}
        if change == "value"
        else {"raw_hash": HASH}
        if change == "raw"
        else {"normalized_hash": HASH}
        if change == "normalized"
        else {"type": "literal"}
    )
    if change in {"value", "raw", "normalized", "type"}:
        hints = (hints[0].model_copy(update=updates),)
    elif change == "missing":
        schema = Schema(slots=())
    else:
        schema = Schema(slots=(schema.slots[0].model_copy(update={"max": 1}),))
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        verify_values("試験２枚", "試験N枚", schema, hints)


def test_repeated_slot_values_must_be_equal_and_slot_positions_cannot_overlap() -> None:
    original_schema, original_hints = numeric_fixture("試験２枚／３枚")
    hints = tuple(h.model_copy(update={"name": "count"}) for h in original_hints)
    schema = Schema(
        slots=(
            Slot(
                name="count",
                type="uint",
                occurrences=tuple(h.occurrence for h in hints),
                reference_kind=None,
                min=0,
                max=SAFE_INTEGER,
            ),
        )
    )
    with pytest.raises(
        ValueError,
        match=r"\ARepeated parameter occurrences must have identical values\Z",
    ):
        verify_values("試験２枚／３枚", "試験N枚／N枚", schema, hints)
    with pytest.raises(ValidationError) as raised:
        Schema(
            slots=(
                original_schema.slots[0],
                original_schema.slots[1].model_copy(
                    update={"occurrences": original_schema.slots[0].occurrences}
                ),
            )
        )

    assert [e["msg"] for e in raised.value.errors()] == [
        "Value error, Parameter occurrences must not overlap"
    ]


@pytest.mark.parametrize(
    ("update", "message"),
    [
        ({"min": None}, "Unsigned slot requires ordered safe integer bounds"),
        ({"max": 0, "min": 1}, "Unsigned slot requires ordered safe integer bounds"),
        ({"reference_kind": "card"}, "Slot reference kind disagrees with its type"),
    ],
)
def test_schema_guards_uint_bounds_and_reference_kind(
    update: dict[str, object], message: str
) -> None:
    schema, _ = numeric_fixture("試験２枚")
    data = dict(schema.slots[0].model_dump(), **update)
    with pytest.raises(ValidationError) as raised:
        Slot.model_validate(data)
    assert [e["msg"] for e in raised.value.errors()] == ["Value error, " + message]
    with pytest.raises(ValidationError) as raised:
        SourceSpan(role="body", segments=(Range(start=0, end=1),), anchor=0)
    assert [e["msg"] for e in raised.value.errors()] == [
        "Value error, Only reminder spans may have an anchor"
    ]


def test_candidate_replay_rejects_changed_literal_positions_and_missing_slots() -> None:
    text = "試験２枚"
    parts = partition(text)
    ref = SourceRef(
        store_id="synthetic",
        batch_id=HASH,
        source_version_id="src:v1:" + "b" * 64,
        parser=PARSER,
        locator="/faces/0/text",
        text_hash=digest(text.encode()),
    )
    item = entry(ref, parts[0], VERSION)
    position = locate(text, parts)[0]
    result = analyze(text, parts[0], item, position, References())
    damaged = result.model_copy(update={"literal_trace": ()})
    with pytest.raises(
        ValueError,
        match=r"\AParameter candidate must replay exact spans roles and semantic evidence\Z",
    ):
        verify_candidate(text, parts[0], item, position, References(), damaged)
    damaged = result.model_copy(
        update={"slots": (), "parameter_schema": Schema(slots=())}
    )
    with pytest.raises(
        ValueError,
        match=r"\AParameter candidate must replay exact spans roles and semantic evidence\Z",
    ):
        verify_candidate(text, parts[0], item, position, References(), damaged)
    with pytest.raises(
        ValueError,
        match=r"\AParameter candidate must match the pinned normalized hash\Z",
    ):
        verify_candidate(
            text,
            replace(parts[0], normalized="Unrelated"),
            item,
            position,
            References(),
            result,
        )


@pytest.mark.parametrize(
    ("text", "rule"),
    [
        ("試験２枚", "suffix_unit_cards"),
        ("試験２体", "suffix_unit_entities"),
        ("試験２点", "suffix_unit_points"),
        ("試験２回", "suffix_unit_times"),
        ("試験２ターン", "suffix_unit_turns"),
        ("試験２PP", "suffix_unit_pp"),
        ("コスト２", "prefix_field_cost"),
        ("攻撃力=２", "prefix_field_attack"),
        ("体力:２", "prefix_field_health"),
        ("PP：２", "prefix_field_pp"),
        ("レベル２", "prefix_field_level"),
        ("コスト２枚", "suffix_unit_cards"),
    ],
)
def test_each_proposed_numeric_rule_is_pinned_and_requires_approval(
    text: str, rule: str
) -> None:
    result = candidate(text)
    assert result.slots[0].numeric_rule == rule
    assert result.slots[0].issues == (NUMERIC_RULE_PENDING,)
    assert result.issues == (NUMERIC_RULE_PENDING,)
    assert result.parameter_schema is None
    assert result.payload_hash is None
    assert result.slots[0].value == 2
    assert rule.isascii()


@pytest.mark.parametrize("text", ["-２枚", "SYN２枚", "試験２", "試験２PPa"])
def test_rejected_numeric_contexts_have_no_proposed_rule(text: str) -> None:
    assert candidate(text).slots[0].numeric_rule is None


def test_header_numeric_roles_do_not_inherit_body_rule_approval() -> None:
    result = candidate(
        "『Name』{Synthetic}フォロワー{コスト２}{攻撃力}１/{体力}３", section=0
    )
    numeric = [h for h in result.slots if h.type == "uint"]
    assert len(numeric) == 3
    assert all(h.numeric_rule is None and not h.issues for h in numeric)


def test_rule_counts_and_conditional_completion_are_distinct_from_complete_schemas() -> (
    None
):
    entries = [
        candidate("試験"),
        candidate("試験２枚／３枚"),
        candidate("コスト２"),
        candidate("試験２枚／３"),
        candidate("試験２枚『Synthetic』"),
    ]
    report = summary(Candidates(entries=entries))
    rules = object_value(report["numeric_rule_counts"])
    assert rules["suffix_unit_cards"] == 4
    assert rules["prefix_field_cost"] == 1
    assert sum(v for v in rules.values() if isinstance(v, int)) == 5
    assert len(rules) == 11
    assert rules["prefix_field_level"] == 0
    body = object_value(object_value(report["roles"])["body"])
    assert body["complete_schemas"] == 1
    assert body["complete_without_numeric_rule_approval"] == 1
    assert body["complete_after_numeric_rule_approval"] == 2
    assert body["review_required"] == 4
    assert report["parameter_complete"] is False
    assert object_value(report["unresolved_reasons"])[NUMERIC_RULE_PENDING] == 4
    assert summary(Candidates(entries=[entries[1]]))["parameter_complete"] is False


def test_rule_identifier_and_pending_reason_are_part_of_exact_candidate_replay() -> (
    None
):
    text = "試験２枚"
    part = partition(text)[0]
    located = locate(text, (part,))[0]
    result = candidate(text)
    ref = SourceRef(
        store_id="synthetic",
        batch_id=HASH,
        source_version_id="src:v1:" + "b" * 64,
        parser=PARSER,
        locator="/faces/0/text",
        text_hash=digest(text.encode()),
    )
    item = entry(ref, part, VERSION)
    for updates in ({"numeric_rule": "suffix_unit_times"}, {"issues": ()}):
        damaged = result.model_copy(
            update={"slots": (result.slots[0].model_copy(update=updates),)}
        )
        with pytest.raises(
            ValueError,
            match=r"\AParameter candidate must replay exact spans roles and semantic evidence\Z",
        ):
            verify_candidate(text, part, item, located, References(), damaged)


def test_numeric_rule_is_in_schema_signature_even_when_slot_shapes_match() -> None:
    cards = candidate("試験２枚")
    entities = candidate("試験２体")
    assert cards.slots[0].occurrence == entities.slots[0].occurrence
    assert cards.signature_hash != entities.signature_hash
    assert cards.signature_hash == candidate("試験３枚").signature_hash


def test_rule_match_does_not_make_an_unsafe_value_conditionally_complete() -> None:
    result = candidate("９００７１９９２５４７４０９９２枚")
    assert result.slots[0].numeric_rule == "suffix_unit_cards"
    report = summary(Candidates(entries=[result]))
    body = object_value(object_value(report["roles"])["body"])
    assert body["complete_schemas"] == 0
    assert body["complete_without_numeric_rule_approval"] == 0
    assert body["complete_after_numeric_rule_approval"] == 0
    assert object_value(report["numeric_rule_counts"])["suffix_unit_cards"] == 1
