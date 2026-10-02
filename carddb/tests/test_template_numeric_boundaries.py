"""Invented lexical counterexamples distinguish quantities, recovery and ordinal indices."""

import pytest
from pydantic import ValidationError

from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.template_parameters.analysis import NUMERIC_RULE_PENDING, NUMERIC_SUFFIX
from sve_carddb.template_parameters.inventory import Candidates, summary
from sve_carddb.template_parameters.models import Hint
from sve_carddb.template_parameters.numeric_rules import (
    ORDINAL_PENDING,
    RECOVERY_PENDING,
    configuration,
)

from .test_template_parameters import candidate

UNITS = ("枚", "体", "点", "回", "ターン", "PP")


@pytest.mark.parametrize("unit", UNITS)
@pytest.mark.parametrize("prefix", ["試験", "コスト", "PP："])
def test_all_unit_ordinals_stay_pending_and_cannot_fall_back_to_prefix(
    unit: str, prefix: str
) -> None:
    result = candidate(f"{prefix}２{unit}目")
    assert len(result.slots) == 1
    hint = result.slots[0]
    assert hint.value == 2
    assert hint.numeric_rule is None
    assert hint.issues == (ORDINAL_PENDING,)
    assert result.issues == (ORDINAL_PENDING,)
    assert result.parameter_schema is None
    assert result.payload_hash is None
    assert NUMERIC_SUFFIX.match(unit + "目") is None


@pytest.mark.parametrize(
    "prefix", ["試験", "コスト", "攻撃力=", "体力:", "PP：", "レベル"]
)
def test_recovery_amount_is_not_repetition_or_any_fallback_field(prefix: str) -> None:
    result = candidate(f"{prefix}４回復")
    assert result.slots[0].value == 4
    assert result.slots[0].numeric_rule is None
    assert result.slots[0].issues == (RECOVERY_PENDING,)
    assert result.parameter_schema is None
    assert result.payload_hash is None
    assert NUMERIC_SUFFIX.match("回復") is None


@pytest.mark.parametrize("unit", UNITS)
def test_genuine_quantity_suffixes_keep_their_existing_pending_rule(unit: str) -> None:
    result = candidate(f"試験２{unit}を")
    assert result.slots[0].numeric_rule is not None
    assert result.issues == (NUMERIC_RULE_PENDING,)


@pytest.mark.parametrize(
    ("text", "reason"),
    [
        ("-２回復", "signed_numeric_requires_review"),
        ("+２枚目", "signed_numeric_requires_review"),
        ("SYN２回復", "numeric_identifier_requires_review"),
        ("SYN２枚目", "numeric_identifier_requires_review"),
        ("試験２つ目", "numeric_role_requires_review"),
    ],
)
def test_vetoes_do_not_bypass_prior_exclusions_or_invent_new_units(
    text: str, reason: str
) -> None:
    result = candidate(text)
    assert result.slots[0].numeric_rule is None
    assert result.issues == (reason,)


def test_vetoed_slots_and_literal_provenance_cannot_disappear_from_summary() -> None:
    entries = [
        candidate("試験"),
        candidate("試験２枚"),
        candidate("試験２回復／３枚目"),
        candidate("試験２回復／４回復"),
    ]
    report = summary(Candidates(entries=entries))
    body = object_value(object_value(report["roles"])["body"])
    assert body["complete_without_numeric_rule_approval"] == 1
    assert body["complete_after_numeric_rule_approval"] == 1
    assert body["review_required"] == 3
    assert report["parameter_complete"] is False
    assert object_value(report["slot_counts"])["numeric"] == 5
    proposals = [
        object_value(p) for p in array(report["inactive_numeric_rule_candidates"])
    ]
    assert [(p["id"], p["positions"], p["uses"]) for p in proposals] == [
        ("candidate_recovery_amount", 3, 2),
        ("candidate_ordinal", 1, 1),
    ]
    assert all(
        p["enabled"] is False and p["status"] == "proposal_only" for p in proposals
    )
    assert len(array(proposals[0]["members"])) == 3
    first = entries[2]
    assert len(first.slots) == 2
    assert len(first.literal_trace) == 3
    assert first.source_span.segments[0].end == len("試験２回復／３枚目")


def test_diagnostic_definitions_are_fixed_ascii_and_never_active_rule_ids() -> None:
    config = configuration()
    assert config["version"] == "numeric-rule-proposals-v3"
    assert config["guard_order"] == [
        "sign",
        "ascii_identifier",
        "lexical_veto",
        "suffix",
        "prefix",
    ]
    specs = [object_value(p) for p in array(config["proposals"])]
    assert specs[0]["suffix_pattern"] == r"^\u56de\u5fa9"
    assert (
        specs[1]["suffix_pattern"]
        == r"^(?:\u679a|\u4f53|\u70b9|\u56de|\u30bf\u30fc\u30f3|PP)\u76ee"
    )
    for spec in specs:
        assert spec["enabled"] is False
        assert spec["status"] == "proposal_only"
        value = candidate("試験２枚").slots[0].model_dump(mode="json")
        value["numeric_rule"] = spec["id"]
        with pytest.raises(ValidationError) as raised:
            Hint.model_validate_json(canonical(value))
        assert len(raised.value.errors()) == 1
        assert raised.value.errors()[0]["loc"] == ("numeric_rule",)
        assert raised.value.errors()[0]["type"] == "literal_error"


@pytest.mark.parametrize("text", ["②回復", "９００７１９９２５４７４０９９２回目"])
def test_lexical_proposals_do_not_authorize_invalid_raw_values(text: str) -> None:
    result = candidate(text)
    assert result.parameter_schema is None
    assert "invalid_safe_unsigned_decimal" in result.slots[0].issues
    assert result.slots[0].numeric_rule is None


def test_excluded_matchers_are_anchored_at_the_position_and_not_general_substrings() -> (
    None
):
    assert candidate("試験２枚の回復目").slots[0].numeric_rule == "suffix_unit_cards"
    assert candidate("試験２回に目").slots[0].numeric_rule == "suffix_unit_times"
    assert candidate("試験２ターンに目").slots[0].numeric_rule == "suffix_unit_turns"


EXPECTED_EXISTING_CONDITION_HASHES = {
    "prefix_field_cost": "sha256:1622b3de948695bfa5470bc78fad9766148ffdcd8fcc27ce2bdd0b0f939e944c",
    "suffix_unit_cards": "sha256:5ae7c9361ad9d4e0b866609be9846822474979c894486b2e77bc7df2d98ac7ee",
    "suffix_unit_entities": "sha256:42529ca9aa528d6fa7f0c7c93b09f26cc8d9f4369aa53b8eec6cbc270892548e",
    "suffix_unit_times": "sha256:634d0a8b9b7b9178e6a47014d7dd12791f72fd66e332127d3dad05d844074ec5",
    "suffix_unit_turns": "sha256:2111bfea1ec41357eda5f4a25a956895b7bcb50698279ca0daa5e4e5a201578a",
    "prefix_field_health": "sha256:2e38b9db593cfbf06fb4534d034807660859a3bc5b780b00379f571982b47f82",
    "prefix_field_attack": "sha256:bacd0862b5b550e569539c19547e499a517ddaa2f40d138248c45bebd37d0c48",
    "suffix_unit_pp": "sha256:1ff95fd5a859140ff12088271d30036fea6d917580bfca142548703526d79b0d",
}


def test_existing_eight_export_matching_conditions_from_the_repository_program() -> (
    None
):
    rows = {
        str(object_value(row)["id"]): object_value(row)
        for row in array(configuration()["rules"])
    }
    for identifier, expected in EXPECTED_EXISTING_CONDITION_HASHES.items():
        wire = rows[identifier]
        assert wire["condition_hash"] == expected
        assert digest(canonical(wire["match_conditions"])) == expected
        assert "status" not in object_value(wire["match_conditions"])
