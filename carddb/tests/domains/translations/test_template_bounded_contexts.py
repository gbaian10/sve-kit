"""Kind counts, multipliers, paired damage caps and keyword aliases need explicit evidence."""

import pytest

from sve_carddb.domains.translations.recognition.explicit_rules import EXPLICIT
from sve_carddb.domains.translations.recognition.keyword_aliases import KEYWORD_ALIASES
from sve_carddb.domains.translations.recognition.references import References

from .test_template_explicit_rules import CASES as FIRST_CASES
from .test_template_parameters import matches
from .test_template_value_rules import CASES as VALUE_CASES

CASES = (
    (
        "distinct_card_name_count",
        "場の試験体のカード名の種類数が２種類以上なら試す。",
        "distinct_card_name_count",
        0,
    ),
    (
        "distinct_original_cost_count",
        "場のカードの元のコストの種類数が２種類以上なら試す。",
        "distinct_original_cost_count",
        0,
    ),
    (
        "damage_count_multiplier",
        "「試験体の数」の２倍のダメージ。",
        "damage_count_multiplier",
        1,
    ),
    (
        "damage_attack_multiplier",
        "「試験体の攻撃力」の２倍のダメージ。",
        "damage_attack_multiplier",
        1,
    ),
    (
        "count_formula_multiplier",
        "Xは「試験体の数の２倍」である。",
        "count_formula_multiplier",
        1,
    ),
    (
        "attack_damage_multiplier",
        "合成例：これが与える「リーダーへの攻撃ダメージ」と「交戦ダメージ」を１７倍にする。",
        "attack_damage_multiplier",
        1,
    ),
)


@pytest.mark.parametrize(
    ("identifier", "text"),
    [
        ("distinct_card_name_count", "場の試験体のカード名が２種類以上なら試す。"),
        (
            "distinct_card_name_count",
            "場の試験体の元のコストの種類数が２種類以上なら試す。",
        ),
        ("distinct_card_name_count", "２種類以上なら試す。"),
        (
            "distinct_card_name_count",
            "場の試験体のカード名の種類数が２枚以上なら試す。",
        ),
        (
            "distinct_original_cost_count",
            "場のカードのコストの種類数が２種類以上なら試す。",
        ),
        (
            "distinct_original_cost_count",
            "場のカードの元のコストの種類数が２種類だけなら試す。",
        ),
        (
            "distinct_original_cost_count",
            "場のカードの元のコストの種類数が２種類以上なら試す。２種類以上なら試す。",
        ),
        ("damage_count_multiplier", "２倍のダメージ。"),
        ("damage_count_multiplier", "「試験体の番号」の２倍のダメージ。"),
        ("damage_count_multiplier", "「試験体の数」の２ダメージ。"),
        ("damage_count_multiplier", "「試験体の数」の２倍の回復。"),
        ("damage_count_multiplier", "「試験体の数」の２倍のダメージ名。"),
        ("damage_attack_multiplier", "「試験体の体力」の２倍のダメージ。"),
        ("damage_attack_multiplier", "「試験体の攻撃力」の２倍の回復。"),
        ("count_formula_multiplier", "Yは「試験体の数の２倍」である。"),
        ("count_formula_multiplier", "Xは「試験体の数の２倍」を選ぶ。"),
        ("count_formula_multiplier", "Xは「試験体の数の２倍」である名。"),
        (
            "attack_damage_multiplier",
            "これが受ける「リーダーへの攻撃ダメージ」と「交戦ダメージ」を２倍にする。",
        ),
        (
            "attack_damage_multiplier",
            "これが与える「リーダーへの攻撃ダメージ」を２倍にする。",
        ),
        (
            "attack_damage_multiplier",
            "これが与える「リーダーへの攻撃ダメージ」と「交戦ダメージ」を２倍にする名。",
        ),
    ],
)
def test_wrong_count_metric_multiplicand_direction_and_continuation_stay_pending(
    identifier: str, text: str
) -> None:
    if text.endswith("。２種類以上なら試す。"):
        rows = matches(text, identifier)
        assert len(rows) == 1
        assert rows[0]["normalized_occurrence"] == {
            "start": text.index("２"),
            "end": text.index("２") + 1,
        }
    else:
        assert matches(text, identifier) == ()


def test_damage_cap_slots_are_distinct_and_both_numbers_require_raw_evidence() -> None:
    text = "これが受ける２以上のダメージを５にする。"
    bound = matches(text, "received_damage_lower_bound")
    assigned = matches(text, "received_damage_assignment")
    assert len(bound) == len(assigned) == 1
    assert (bound[0]["value"], bound[0]["recognized_role"]) == (
        2,
        "received_damage_lower_bound",
    )
    assert (assigned[0]["value"], assigned[0]["recognized_role"]) == (
        5,
        "received_damage_assigned_value",
    )
    assert bound[0]["slot"] != assigned[0]["slot"]
    for identifier in ("received_damage_lower_bound", "received_damage_assignment"):
        for bad in ("N", "②", "9007199254740992", "X"):
            assert matches(text.replace("２", bad), identifier) == ()
            assert matches(text.replace("５", bad), identifier) == ()
        assert matches(text.replace("受ける", "与える"), identifier) == ()
        assert matches(text.replace("ダメージ", "回復"), identifier) == ()
        assert matches(text.replace("にする。", "にする名。"), identifier) == ()
        assert matches(text.replace("以上", "以下"), identifier) == ()
        assert matches(text.replace("５", "０"), identifier)
    assert set(EXPLICIT) == {c[0] for c in FIRST_CASES + VALUE_CASES + CASES} | {
        "received_damage_lower_bound",
        "received_damage_assignment",
    }


def evidence() -> References:
    return References(
        terms={
            "ネクロチャージ": [("term:ability.necrocharge", "ability")],
            "スペルチェイン": [("term:ability.spell_chain", "ability")],
        }
    )


@pytest.mark.parametrize(
    ("identifier", "alias", "target", "role"),
    [
        ("keyword_alias_nc", "NC", "term:ability.necrocharge", "necrocharge_threshold"),
        ("keyword_alias_sc", "SC", "term:ability.spell_chain", "spell_chain_threshold"),
    ],
)
def test_alias_threshold_requires_closed_raw_prefix_and_registered_full_ability(
    identifier: str, alias: str, target: str, role: str
) -> None:
    text = "【" + alias + "_２】試す。"
    rows = matches(text, identifier, evidence())
    assert len(rows) == 1
    row = rows[0]
    assert (
        row["value"],
        row["recognized_role"],
        row["target_id"],
    ) == (2, role, target)
    for bad in ("②", "+２", "-２", "9007199254740992", "２猫", "２ "):
        assert matches(text.replace("２", bad), identifier, evidence()) == ()
    for bad in (alias.lower(), "X" + alias, "ＮＣ" if alias == "NC" else "ＳＣ"):
        assert matches(text.replace(alias, bad), identifier, evidence()) == ()
    assert matches(text.replace("_", "＿"), identifier, evidence()) == ()
    assert matches(text.replace("】", ""), identifier, evidence()) == ()
    assert matches(text.replace("２", "０"), identifier, evidence())
    assert matches("『" + text + "』", identifier, evidence()) == ()
    full = KEYWORD_ALIASES[identifier].full_name
    for raw in (
        References(),
        References(terms={alias: [(target, "ability")]}),
        References(terms={full: [(target, "trait")]}),
        References(terms={full: [("term:ability.quick", "ability")]}),
        References(terms={full: [(target, "ability")] * 2}),
    ):
        assert matches(text, identifier, raw) == ()
