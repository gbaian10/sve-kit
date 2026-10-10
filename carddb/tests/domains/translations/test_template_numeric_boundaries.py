"""Invented lexical counterexamples distinguish quantities, recovery and ordinal indices."""

import pytest
from pydantic import ValidationError

from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains.translations.recognition.analysis import (
    NUMERIC_RULE_DISABLED,
    NUMERIC_SUFFIX,
)
from sve_carddb.domains.translations.recognition.models import Hint
from sve_carddb.domains.translations.recognition.numeric_rules import (
    NUMERIC_RULES,
    ORDINAL_PENDING,
    conditions,
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
    assert NUMERIC_SUFFIX.match(unit + "目") is None


@pytest.mark.parametrize("unit", UNITS)
def test_genuine_quantity_suffixes_keep_their_existing_pending_rule(unit: str) -> None:
    result = candidate(f"試験２{unit}を")
    assert result.slots[0].numeric_rule is not None
    assert result.issues == (NUMERIC_RULE_DISABLED,)


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
    assert "invalid_safe_unsigned_decimal" in result.slots[0].issues
    assert result.slots[0].numeric_rule is None


def test_excluded_matchers_are_anchored_at_the_position_and_not_general_substrings() -> (
    None
):
    assert candidate("試験２枚の回復目").slots[0].numeric_rule == "suffix_unit_cards"
    assert candidate("試験２回に目").slots[0].numeric_rule == "suffix_unit_times"
    assert candidate("試験２ターンに目").slots[0].numeric_rule == "suffix_unit_turns"


def test_numeric_definitions_expose_conditions_without_approval_wrappers() -> None:
    rows = [object_value(row) for row in array(configuration()["rules"])]
    assert [row["id"] for row in rows] == list(NUMERIC_RULES)
    for rule, row in zip(NUMERIC_RULES, rows, strict=True):
        assert row["match_conditions"] == conditions(rule)
        assert "condition_hash" not in row
        assert "status" not in row
