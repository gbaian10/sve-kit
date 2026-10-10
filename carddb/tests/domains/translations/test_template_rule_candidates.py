"""Invented counterexamples for opt-in lexical recognition without approval or adoption."""

import copy
import re
from dataclasses import replace

import pytest

from sve_carddb.core.json import array, object_value
from sve_carddb.domains.translations.recognition import candidate_matching as matching
from sve_carddb.domains.translations.recognition.candidate_matching import recognize
from sve_carddb.domains.translations.recognition.references import References
from sve_carddb.domains.translations.recognition.rule_candidates import (
    BY_ID,
    RULES,
    SUFFIXES,
    conditions,
    configuration,
    definition,
    selection,
)

from .test_template_parameters import candidate, matches, partition

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
    assert rows[0]["recognized_role"] == role
    assert rows[0]["source_segments"] == [{"start": 2, "end": 3}]
    assert rows[0]["value"] == 2
    assert value.model_dump() == before
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


@pytest.mark.parametrize("rule", ["suffix_damage_amount", "leader_person_quantity"])
@pytest.mark.parametrize(
    ("text", "reason"),
    [
        ("ID_２ダメージ", "numeric_identifier_requires_review"),
        ("+２ダメージ", "signed_numeric_requires_review"),
    ],
)
def test_general_matchers_cannot_claim_vetoed_positions_even_if_grammar_matches(
    monkeypatch: pytest.MonkeyPatch, rule: str, text: str, reason: str
) -> None:
    value = candidate(text)
    assert value.slots[0].issues == (reason,)
    monkeypatch.setattr(matching, "_match", lambda *_: matching.Match())
    assert recognize(text, partition(text)[0], value, References(), (rule,)) == ()


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
    assert partition("（試験" + sign + "２枚）")[0].role == "reminder"
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
        terms={marker[1:-1]: [(target[0], "rule_term")]} if target else {}
    )
    text = marker + sign + "２を試す"
    value = candidate(text, refs)
    original = value.model_dump()
    rows = matches(text, rule, refs)
    assert len(rows) == 1
    assert (
        rows[0]["recognized_role"]
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
        wrong.terms[marker[1:-1]] = [(target[0], "ability")]
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
    refs = References(terms={"SyntheticAbility": [(identifier, "ability")]})
    text = "【SyntheticAbility_２】"
    rows = matches(text, rule, refs)
    assert len(rows) == 1
    assert rows[0]["target_id"] == identifier
    assert (
        rows[0]["recognized_role"]
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
            References(terms={"SyntheticAbility": [(identifier, "rule_term")]}),
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
                        (identifier, "ability"),
                        ("term:ability.other", "ability"),
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
                terms={"SyntheticAbility": [("term:ability.earth_rite", "ability")]}
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
        refs = References(terms={"SyntheticTerm": [(identifier, category)]})
        rows = matches("{SyntheticTerm}試験", rule, refs)
        assert len(rows) == 1
        assert rows[0]["target_id"] == identifier
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
                References(terms={"SyntheticTerm": [("term:ability.other", category)]}),
            )
            == ()
        )
        assert (
            matches(
                "{SyntheticTerm}",
                rule,
                References(terms={"SyntheticTerm": [(identifier, "other")]}),
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
                            (identifier, category),
                            (identifier, category),
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
    assert rows[0]["recognized_role"] == "choice_ordinal"
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
    assert len(BY_ID) == 56
    config = configuration()
    assert config["enabled"] == []
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
        matches("コスト２回復", "suffix_recovery_amount")[0]["recognized_role"]
        == "recovery_amount"
    )


def test_definitions_expose_exact_rule_syntax_and_closed_set(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    for rule in RULES:
        wire = [object_value(r) for r in array(configuration()["rules"])]
        expected = next(r for r in wire if r["id"] == rule.id)
        assert expected["match_conditions"] == conditions(rule)
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
    assert value.slots[0].issues == ("numeric_rule_disabled",)
    assert matches(text, "suffix_damage_amount") == ()


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
        terms={marker[1:-1]: [(target[0], "rule_term")]} if target else {}
    )
    text = "（" + marker + sign + "２）"
    value = candidate(text, refs)
    assert partition("（試験" + sign + "２枚）")[0].role == "reminder"
    assert value.slots[-1].issues == ("signed_numeric_requires_review",)
    assert matches(text, rule, refs) == ()
    assert (
        len(matches(text.replace(sign, "+" if sign == "＋" else "-"), rule, refs)) == 1
    )


def test_display_description_does_not_change_matching_conditions() -> None:
    for rule in RULES:
        renamed = replace(rule, condition="Edited display explanation")
        assert conditions(renamed) == conditions(rule)
        wire = definition(rule)
        assert "status" not in object_value(wire["match_conditions"])
        assert "condition" not in object_value(wire["match_conditions"])


def test_rule_specific_conditions_exclude_unrelated_family_syntax(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    before = {rule.id: conditions(rule) for rule in RULES}
    monkeypatch.setattr(
        "sve_carddb.domains.translations.recognition.rule_candidates.INTRO_PATTERN",
        "altered choice introduction",
    )
    after = {rule.id: conditions(rule) for rule in RULES}
    assert {key for key in before if before[key] != after[key]} == {
        "bracket_choice_index"
    }
