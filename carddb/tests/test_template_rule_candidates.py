"""Invented counterexamples for opt-in lexical recognition without approval or adoption."""

import copy
import re
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.snapshot.values import array, canonical, digest, integer, object_value
from sve_carddb.template_parameters import candidate_matching as matching
from sve_carddb.template_parameters.candidate_matching import recognize
from sve_carddb.template_parameters.inventory import Candidates, Field, _field, summary
from sve_carddb.template_parameters.references import References
from sve_carddb.template_parameters.rule_candidates import (
    BY_ID,
    RULES,
    SUFFIXES,
    condition_hash,
    conditions,
    configuration,
    definition,
    selection,
)
from sve_carddb.template_sources.inventory import entry
from sve_carddb.template_sources.normalizer import VERSION, partition
from sve_carddb.template_sources.pins import PARSER

from .test_template_parameters import HASH, candidate

if TYPE_CHECKING:
    from pydantic import JsonValue


def matches(
    text: str, rule: str, refs: References | None = None
) -> tuple[dict[str, JsonValue], ...]:
    evidence = refs or References()
    value = candidate(text, evidence)
    return recognize(text, partition(text)[0], value, evidence, (rule,))


SUFFIX_CASES = (
    ("suffix_damage_amount", "試験２ダメージ", "試験２ダメージX", "damage_amount"),
    ("suffix_recovery_amount", "試験２回復", "試験２回", "recovery_amount"),
    ("suffix_ordinal_cards", "試験２枚目", "試験２枚", "card_ordinal"),
    ("suffix_ordinal_times", "試験２回目", "試験２回復", "repetition_ordinal"),
    ("suffix_ordinal_turns", "試験２ターン目", "試験２ターン", "turn_ordinal"),
    ("suffix_unit_items", "試験２つを試す", "試験２つながる", "item_quantity"),
)


@pytest.mark.parametrize(("rule", "positive", "negative", "role"), SUFFIX_CASES)
def test_each_suffix_is_opt_in_and_cannot_authorize_or_change_a_candidate(
    rule: str, positive: str, negative: str, role: str
) -> None:
    value = candidate(positive)
    before = value.model_dump()
    assert recognize(positive, partition(positive)[0], value, References()) == ()
    rows = matches(positive, rule)
    assert len(rows) == 1
    assert rows[0]["rule_id"] == rule
    assert rows[0]["proposed_role"] == role
    assert rows[0]["status"] == "pending_approval"
    assert rows[0]["source_segments"] == [{"start": 2, "end": 3}]
    assert rows[0]["value"] == 2
    assert value.model_dump() == before
    assert value.parameter_schema is None
    assert value.payload_hash is None
    assert value.issues
    assert matches(negative, rule) == ()


@pytest.mark.parametrize("unit", ["枚", "回", "ターン"])
def test_ordinal_zero_and_zero_instance_units_never_receive_a_proposal(
    unit: str,
) -> None:
    rule = {
        "枚": "suffix_ordinal_cards",
        "回": "suffix_ordinal_times",
        "ターン": "suffix_ordinal_turns",
    }[unit]
    assert matches("試験０" + unit + "目", rule) == ()
    for absent in ("体", "点", "PP"):
        assert matches("試験２" + absent + "目", rule) == ()


@pytest.mark.parametrize(("rule", "positive", "negative", "role"), SUFFIX_CASES)
@pytest.mark.parametrize("prefix", ["+", "-", "−", "＋", "－", "ID_", "Q"])
def test_suffixes_do_not_remove_sign_or_identifier_guards(
    rule: str, positive: str, negative: str, role: str, prefix: str
) -> None:
    del negative, role
    assert matches(prefix + positive[2:], rule) == ()


@pytest.mark.parametrize(("rule", "positive", "negative", "role"), SUFFIX_CASES)
def test_invalid_raw_digit_or_overflow_never_becomes_a_proposal(
    rule: str, positive: str, negative: str, role: str
) -> None:
    del negative, role
    assert matches(positive.replace("２", "②"), rule) == ()
    assert matches(positive.replace("２", "9007199254740992"), rule) == ()


@pytest.mark.parametrize("sign", ["＋", "－"])
def test_unchanged_reminder_fullwidth_sign_cannot_hit_an_old_unit(sign: str) -> None:
    value = candidate("（試験" + sign + "２枚）")
    assert value.source_span.role == "reminder"
    assert value.slots[0].numeric_rule is None
    assert value.slots[0].issues == ("signed_numeric_requires_review",)


@pytest.mark.parametrize(
    "rule", ["prefix_attack_delta", "prefix_health_delta", "prefix_cost_delta"]
)
@pytest.mark.parametrize("sign", ["+", "-"])
def test_signed_roles_keep_sign_literal_and_magnitude_unsigned(
    rule: str, sign: str
) -> None:
    marker = {
        "prefix_attack_delta": "{攻撃力}",
        "prefix_health_delta": "{体力}",
        "prefix_cost_delta": "コストを",
    }[rule]
    target = BY_ID[rule].targets
    refs = References(
        terms={marker[1:-1]: [(target[0], "rule_term", HASH)]} if target else {}
    )
    text = marker + sign + "２を試す"
    value = candidate(text, refs)
    original = value.model_dump()
    rows = matches(text, rule, refs)
    assert len(rows) == 1
    assert (
        rows[0]["proposed_role"]
        == {
            "prefix_attack_delta": "attack_delta_magnitude",
            "prefix_health_delta": "health_delta_magnitude",
            "prefix_cost_delta": "cost_delta_magnitude",
        }[rule]
    )
    assert rows[0]["value"] == 2
    assert rows[0]["source_segments"] == [
        {"start": len(marker) + 1, "end": len(marker) + 2}
    ]
    assert any(
        text[s.start : s.end].endswith(sign)
        for literal in value.literal_trace
        for s in literal.source_segments
    )
    assert value.model_dump() == original
    assert matches(marker + ":" + sign + "２", rule, refs) == ()
    assert matches(marker + sign * 2 + "２", rule, refs) == ()
    assert matches(marker + "−２", rule, refs) == ()
    assert matches(marker + sign + "２X", rule, refs) == ()
    assert matches("未知" + sign + "２", rule, refs) == ()
    assert matches(marker + sign + "9007199254740992", rule, refs) == ()
    if target:
        assert matches(text, rule) == ()
        wrong = copy.deepcopy(refs)
        wrong.terms[marker[1:-1]] = [(target[0], "ability", HASH)]
        assert matches(text, rule, wrong) == ()


@pytest.mark.parametrize(
    "rule", [r for r in BY_ID if r.startswith("keyword_threshold_")]
)
def test_each_threshold_requires_complete_exact_unique_adopted_concept(
    rule: str,
) -> None:
    identifier = BY_ID[rule].targets[0]
    assert (
        identifier
        == {
            "keyword_threshold_combo": "term:ability.combo",
            "keyword_threshold_lesson": "term:ability.lesson",
            "keyword_threshold_necrocharge": "term:ability.necrocharge",
            "keyword_threshold_spell_chain": "term:ability.spell_chain",
        }[rule]
    )
    refs = References(terms={"SyntheticAbility": [(identifier, "ability", HASH)]})
    text = "【SyntheticAbility_２】"
    rows = matches(text, rule, refs)
    assert len(rows) == 1
    assert rows[0]["target_id"] == identifier
    assert rows[0]["target_hash"] == HASH
    assert (
        rows[0]["proposed_role"]
        == {
            "keyword_threshold_combo": "combo_threshold",
            "keyword_threshold_lesson": "lesson_threshold",
            "keyword_threshold_necrocharge": "necrocharge_threshold",
            "keyword_threshold_spell_chain": "spell_chain_threshold",
        }[rule]
    )
    for negative in (
        "SyntheticAbility_２】",
        "【SyntheticAbility２】",
        "【SyntheticAbility_２",
        "【SyntheticAbility_+２】",
        "【SyntheticAbility_２X】",
        "【OtherAbility_２】",
        "【SyntheticAbility_9007199254740992】",
    ):
        assert matches(negative, rule, refs) == ()
    assert matches(text, rule) == ()
    assert (
        matches(
            text,
            rule,
            References(terms={"SyntheticAbility": [(identifier, "rule_term", HASH)]}),
        )
        == ()
    )
    assert (
        matches(
            text,
            rule,
            References(
                terms={
                    "SyntheticAbility": [
                        (identifier, "ability", HASH),
                        ("term:ability.other", "ability", HASH),
                    ]
                }
            ),
        )
        == ()
    )
    assert (
        matches(
            text,
            rule,
            References(
                terms={
                    "SyntheticAbility": [("term:ability.earth_rite", "ability", HASH)]
                }
            ),
        )
        == ()
    )
    # Normalization may preserve shape, but must not manufacture an adopted raw name.
    assert matches("【ＳyntheticAbility_２】", rule, refs) == ()


@pytest.mark.parametrize(
    "rule",
    [
        "braced_stat_reference",
        "braced_ability_reference",
        "braced_action_engage_reference",
    ],
)
def test_every_closed_term_member_is_exactly_bound_without_inferring_effects(
    rule: str,
) -> None:
    for identifier in BY_ID[rule].targets:
        category = "rule_term" if rule == "braced_stat_reference" else "ability"
        refs = References(terms={"SyntheticTerm": [(identifier, category, HASH)]})
        rows = matches("{SyntheticTerm}試験", rule, refs)
        assert len(rows) == 1
        assert rows[0]["target_id"] == identifier
        assert rows[0]["target_hash"] == HASH
        assert rows[0]["value"] is None
        for bad in (
            "SyntheticTerm試験",
            "{SyntheticTerm試験",
            "{OtherTerm}試験",
            "『{SyntheticTerm}』",
            "{{SyntheticTerm}}",
            "{Other{SyntheticTerm}Other}",
        ):
            assert matches(bad, rule, refs) == ()
        assert (
            matches(
                "{SyntheticTerm}",
                rule,
                References(
                    terms={"SyntheticTerm": [("term:ability.other", category, HASH)]}
                ),
            )
            == ()
        )
        assert (
            matches(
                "{SyntheticTerm}",
                rule,
                References(terms={"SyntheticTerm": [(identifier, "other", HASH)]}),
            )
            == ()
        )
        assert (
            matches(
                "{SyntheticTerm}",
                rule,
                References(
                    terms={
                        "SyntheticTerm": [
                            (identifier, category, HASH),
                            (identifier, category, HASH),
                        ]
                    }
                ),
            )
            == ()
        )
        assert matches("{ＳyntheticTerm}", rule, refs) == ()


def test_choice_requires_same_field_earlier_body_introduction_and_all_option_labels() -> (
    None
):
    text = "２つチョイス。試験【１】仮【２】例"
    rows = matches(text, "bracket_choice_index")
    assert len(rows) == 2
    assert rows[0]["proposed_role"] == "choice_ordinal"
    assert rows[0]["context_segments"] == [
        {"start": 0, "end": 6},
        {"start": 9, "end": 12},
        {"start": 13, "end": 16},
    ]
    for bad in (
        "【１】仮【２】例",
        "【１】仮【２】例２つチョイス",
        "２つチョイス【１】仮",
        "２つチョイス【１】仮【１】例",
        "２つチョイス【１】仮【３】例",
        "２つチョイス【２】仮【１】例",
        "２つチョイス【０】仮【１】例",
        "（２つチョイス）【１】仮【２】例",
        "ID_２つチョイス【１】仮【２】例",
        "『２つチョイス』【１】仮【２】例",
    ):
        assert matches(bad, "bracket_choice_index") == ()


@pytest.mark.parametrize(
    "tail",
    [
        "チョイス",
        "まで",
        "を試す",
        "持つ",
        "裏向き",
        "）",
        ")",
        "以上",
        "につき",
        "の仮",
        "。",
    ],
)
def test_each_generic_item_continuation_has_an_independent_example(tail: str) -> None:
    assert len(matches("試験２つ" + tail, "suffix_unit_items")) == 1
    assert matches("試験２つ目" + tail, "suffix_unit_items") == ()


@pytest.mark.parametrize(
    ("enabled", "message"),
    [
        (
            ("suffix_damage_amount", "suffix_damage_amount"),
            "Candidate rule selection must be unique",
        ),
        (("unknown",), "Candidate rule selection contains an unknown rule"),
    ],
)
def test_switches_cannot_implicitly_expand_to_unknown_or_duplicate_rules(
    enabled: tuple[str, ...], message: str
) -> None:
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        selection(enabled)


def test_registry_is_closed_default_off_and_not_an_approval_receipt() -> None:
    assert len(BY_ID) == 36
    config = configuration()
    assert config["enabled"] == []
    assert config["recognition_policy"] is None
    assert config["status"] == "pending_approval"
    assert all("earth_rite" not in rule for rule in BY_ID)
    assert BY_ID["braced_stat_reference"].targets == (
        "term:stat.attack",
        "term:stat.health",
    )
    assert set(BY_ID["braced_ability_reference"].targets) == {
        "term:ability.fanfare",
        "term:ability.activation",
        "term:ability.last_words",
        "term:ability.feed",
        "term:ability.union_burst",
        "term:ability.quick",
        "term:ability.possession",
        "term:ability.advance_activation",
    }
    assert BY_ID["braced_action_engage_reference"].targets == ("term:action.engage",)
    assert configuration(("suffix_damage_amount",))["enabled"] == [
        "suffix_damage_amount"
    ]
    assert (
        matches("コスト２回復", "suffix_recovery_amount")[0]["proposed_role"]
        == "recovery_amount"
    )


def test_field_replays_prior_line_choice_context_and_emits_hash_only_proposals() -> (
    None
):
    text = "２つチョイス。\n【１】仮\n【２】例"
    ref = SourceRef(
        batch_id=HASH,
        source_version_id="src:v1:" + "b" * 64,
        parser=PARSER,
        locator="/faces/0/text",
        text_hash=digest(text.encode()),
    )
    context = Field(
        "test-store",
        ref.source_version_id,
        ref.locator,
        text,
        None,
        [entry(ref, p, VERSION, store_id="test-store") for p in partition(text)],
        {"faces": [{"text": text, "sections": []}]},
    )
    off = Candidates()
    on = Candidates(enabled_rules=("bracket_choice_index", "suffix_unit_items"))
    _field(off, context, References())
    _field(on, context, References())
    assert on.entries == off.entries
    assert off.rule_matches == []
    choices = [r for r in on.rule_matches if r["rule_id"] == "bracket_choice_index"]
    assert len(choices) == 2
    assert len({r["inventory_id"] for r in choices}) == 2
    assert all(
        array(r["context_segments"])[0] == {"start": 0, "end": 6} for r in choices
    )
    counts = object_value(summary(on)["candidate_rule_counts"])
    assert counts["bracket_choice_index"] == {
        "positions": 2,
        "uses": 2,
        "body_positions": 2,
        "body_uses": 2,
    }
    assert summary(on)["parameter_complete"] is False
    assert all("仮".encode() not in canonical(row) for row in on.rule_matches)


def test_every_condition_hash_binds_exact_rule_syntax_and_closed_set(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    for rule in RULES:
        wire = [object_value(r) for r in array(configuration()["rules"])]
        expected = next(r for r in wire if r["id"] == rule.id)
        assert expected["condition_hash"] == digest(canonical(conditions(rule)))
    previous = configuration()
    monkeypatch.setitem(SUFFIXES, "suffix_unit_items", r"^つ")
    assert configuration() != previous
    old = candidate("試験２枚")
    assert (
        recognize("試験２枚", partition("試験２枚")[0], old, References(), tuple(BY_ID))
        == ()
    )


def test_even_overlapping_proposals_cannot_share_a_position(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(matching, "_match", lambda *_: matching.Match())
    text = "試験２つを試す"
    with pytest.raises(
        ValueError, match=r"\ACandidate matchers must not share slot ownership\Z"
    ):
        recognize(
            text,
            partition(text)[0],
            candidate(text),
            References(),
            ("suffix_unit_items", "suffix_damage_amount"),
        )


def test_adjacent_ascii_resource_prefix_stays_pending_even_after_lexical_veto() -> None:
    assert candidate("PP２回復").slots[0].issues == (
        "numeric_recovery_amount_requires_review",
    )
    assert matches("PP２回復", "suffix_recovery_amount") == ()


def test_mismatched_source_decimal_and_candidate_value_cannot_be_emitted() -> None:
    text = "試験２ダメージ"
    value = candidate(text)
    damaged = value.model_copy(
        update={"slots": (value.slots[0].model_copy(update={"value": 3}),)}
    )
    assert (
        recognize(
            text, partition(text)[0], damaged, References(), ("suffix_damage_amount",)
        )
        == ()
    )


@pytest.mark.parametrize("prefix", ["コスト", "PP", "攻撃力"])
def test_new_suffix_cannot_claim_an_old_prefix_owned_position(prefix: str) -> None:
    text = prefix + "２ダメージ"
    value = candidate(text)
    assert value.slots[0].numeric_rule is not None
    assert value.slots[0].issues == ("numeric_rule_pending_approval",)
    assert matches(text, "suffix_damage_amount") == ()


def choice_field(text: str) -> Candidates:
    ref = SourceRef(
        batch_id=HASH,
        source_version_id="src:v1:" + "b" * 64,
        parser=PARSER,
        locator="/faces/0/text",
        text_hash=digest(text.encode()),
    )
    field = Field(
        "test-store",
        ref.source_version_id,
        ref.locator,
        text,
        None,
        [entry(ref, part, VERSION, store_id="test-store") for part in partition(text)],
        {"faces": [{"text": text, "sections": []}]},
    )
    result = Candidates(enabled_rules=("bracket_choice_index",))
    _field(result, field, References())
    return result


@pytest.mark.parametrize(
    "text",
    [
        "（２つチョイス）【１】仮【２】例",
        "２つチョイス（【１】仮【２】例）",
        "２つチョイス【１】仮（【２】例）",
    ],
)
def test_choice_introduction_and_labels_must_be_body_spans(text: str) -> None:
    result = choice_field(text)
    assert any(item.source_span.role == "body" for item in result.entries)
    assert any(item.slots for item in result.entries)
    assert result.rule_matches == []


@pytest.mark.parametrize("second_complete", [True, False])
def test_each_choice_introduction_owns_only_its_following_group(
    second_complete: bool,
) -> None:
    first = "２つチョイス【１】仮【２】例。"
    second = "２つチョイス【１】仮" + ("【２】例" if second_complete else "")
    result = choice_field(first + second)
    assert len(result.rule_matches) == (4 if second_complete else 2)
    first_rows = [
        row
        for row in result.rule_matches
        if array(row["context_segments"])[0] == {"start": 0, "end": 6}
    ]
    assert len(first_rows) == 2
    assert all(
        integer(object_value(array(row["context_segments"])[-1])["end"]) <= len(first)
        for row in first_rows
    )
    if second_complete:
        later = result.rule_matches[2:]
        assert all(
            array(row["context_segments"])[0]
            == {"start": len(first), "end": len(first) + 6}
            for row in later
        )


@pytest.mark.parametrize(
    "rule",
    [
        "suffix_recovery_amount",
        "suffix_ordinal_cards",
        "suffix_ordinal_times",
        "suffix_ordinal_turns",
    ],
)
@pytest.mark.parametrize("tail", ["X", "1", "_"])
def test_recovery_and_ordinals_exclude_ascii_continuation(rule: str, tail: str) -> None:
    positive = next(row[1] for row in SUFFIX_CASES if row[0] == rule)
    assert len(matches(positive, rule)) == 1
    assert matches(positive + tail, rule) == ()


@pytest.mark.parametrize(
    "rule", ["prefix_attack_delta", "prefix_health_delta", "prefix_cost_delta"]
)
@pytest.mark.parametrize("sign", ["＋", "－"])
def test_fullwidth_reminder_sign_cannot_enable_a_new_signed_rule(
    rule: str, sign: str
) -> None:
    marker = {
        "prefix_attack_delta": "{攻撃力}",
        "prefix_health_delta": "{体力}",
        "prefix_cost_delta": "コストを",
    }[rule]
    target = BY_ID[rule].targets
    refs = References(
        terms={marker[1:-1]: [(target[0], "rule_term", HASH)]} if target else {}
    )
    text = "（" + marker + sign + "２）"
    value = candidate(text, refs)
    assert value.source_span.role == "reminder"
    assert value.slots[-1].issues == ("signed_numeric_requires_review",)
    assert matches(text, rule, refs) == ()
    assert (
        len(matches(text.replace(sign, "+" if sign == "＋" else "-"), rule, refs)) == 1
    )


def test_display_description_and_status_do_not_change_condition_hash() -> None:
    for rule in RULES:
        renamed = replace(rule, condition="Edited display explanation")
        assert condition_hash(renamed) == condition_hash(rule)
        wire = definition(rule)
        assert "status" not in object_value(wire["match_conditions"])
        assert "condition" not in object_value(wire["match_conditions"])


def test_rule_specific_conditions_do_not_hash_unrelated_family_syntax(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    before = {rule.id: condition_hash(rule) for rule in RULES}
    monkeypatch.setattr(
        "sve_carddb.template_parameters.rule_candidates.INTRO_PATTERN",
        "altered choice introduction",
    )
    after = {rule.id: condition_hash(rule) for rule in RULES}
    assert {key for key in before if before[key] != after[key]} == {
        "bracket_choice_index"
    }


EXPECTED_CONDITION_HASHES = {
    "suffix_damage_amount": "sha256:767777ccad7a4fa402d08d0c9c38ccd61711273265cb7d0bad28924c403d66ab",
    "suffix_recovery_amount": "sha256:c8cde2421c616e3a114560edbcfdd20218b41045f0a4d372fc2d735620a5dc78",
    "suffix_ordinal_cards": "sha256:373ca2b48672aa3812cfbfedb80d42c009419ac16015ba3a2aa9435ab7615693",
    "suffix_ordinal_times": "sha256:49b4eaaaedb37561dc02c12690ae07a6f09ff5c7a809a2043d2f7699fb28856d",
    "suffix_ordinal_turns": "sha256:ecacc3819114aa0e00060a4d157b0c56a7e93d57529f2456ca16187cdc7e598f",
    "suffix_unit_items": "sha256:671ff56a6f73d9ee9c8b5016ddeb1681d7ff0eac8570e1c49682554c13b1a7a7",
    "bracket_choice_index": "sha256:08bbde239098ae434b2204df7d696e739731a23430d7748f3862821a6dc0279a",
    "prefix_attack_delta": "sha256:9bafbb438c1e5caac1e5a3cdabdf3078101e71eeae2539a01abb460e8b4f8d5d",
    "prefix_health_delta": "sha256:f63edcd0900f9ca6d890e5014ba77574707c910712457631c6b7a21ff8ea9695",
    "prefix_cost_delta": "sha256:7c5ba71551c05b8e99359ae6e854c6318936b185e0bc5e06c1f893b9bcbde8d3",
    "keyword_threshold_combo": "sha256:8a7a19026ff68847006c9e737d193063876d2382e17dae44478672b3b42e85f9",
    "keyword_threshold_lesson": "sha256:2e4606cccb807e7e23c0422832278e4c3ddbd8782a588a63bb72d0c93268f3ea",
    "keyword_threshold_necrocharge": "sha256:587c2a354e12efa580fd44cb4b5145c006e4501aec6d5d5cffe36ec20af5c64d",
    "keyword_threshold_spell_chain": "sha256:b6a2a9ada0a15a28fa63c677216a18b3f980db24d3ab49ba0da70ed6399b3230",
    "braced_stat_reference": "sha256:d6ad8e62c6fab8658fa56b38f1e41e4bf8bfa7eb252d389c0ff5a7acb0d2b0de",
    "braced_ability_reference": "sha256:ea38f1ea3e9e900673c9d418583c52a9b477767f9bf0449defd67cde09a912a6",
    "braced_action_engage_reference": "sha256:ddb8e91e2ac760c2cb8d69a52a26b7dc3fe8381a6df3d8bd891b4385081abaec",
}


def test_all_17_condition_hashes_are_pinned_to_reviewed_matching_conditions() -> None:
    assert {
        rule.id: condition_hash(rule)
        for rule in RULES
        if rule.id in EXPECTED_CONDITION_HASHES
    } == EXPECTED_CONDITION_HASHES
