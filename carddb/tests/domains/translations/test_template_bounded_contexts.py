"""Kind counts, multipliers, paired damage caps and keyword aliases need explicit evidence."""

import pytest

from sve_carddb.contracts.template_parameters import Schema, Slot
from sve_carddb.domains.translations.parameters.candidate_matching import (
    classify,
    recognize,
)
from sve_carddb.domains.translations.parameters.explicit_rules import EXPLICIT
from sve_carddb.domains.translations.parameters.keyword_aliases import KEYWORD_ALIASES
from sve_carddb.domains.translations.parameters.references import References
from sve_carddb.domains.translations.parameters.spans import locate
from sve_carddb.domains.translations.source_inventory.inventory import entry
from sve_carddb.domains.translations.source_inventory.normalizer import (
    VERSION,
    partition,
)
from sve_carddb.domains.translations.templates.members import _members

from .test_template_explicit_rules import CASES as FIRST_CASES
from .test_template_explicit_rules import entry_ref
from .test_template_explicit_rules import (
    test_current_resolution_carries_role_and_rejects_weakened_numeric_bounds as check_resolution,
)
from .test_template_explicit_rules import (
    test_explicit_match_requires_opt_in_exact_value_and_preserves_proposal as check_match,
)
from .test_template_parameters import candidate
from .test_template_rule_candidates import matches
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
        "これが与える「リーダーへの攻撃ダメージ」と「交戦ダメージ」を２倍にする。",
        "attack_damage_multiplier",
        1,
    ),
)


@pytest.mark.parametrize(("identifier", "text", "role", "minimum"), CASES)
def test_bounded_contexts_preserve_opt_in_raw_value_role_and_schema(
    identifier: str, text: str, role: str, minimum: int
) -> None:
    check_match(identifier, text, role, minimum)
    check_resolution(identifier, text, role, minimum)
    if minimum == 1:
        assert matches(text.replace("２", "０"), identifier) == ()
    else:
        assert matches(text.replace("２", "０"), identifier)
    assert matches(text.replace("２", "２猫"), identifier) == ()


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


@pytest.mark.parametrize(
    ("text", "identifiers", "roles", "refs"),
    [
        (
            "これが受ける２以上のダメージを５にする。",
            ("received_damage_lower_bound", "received_damage_assignment"),
            ("received_damage_lower_bound", "received_damage_assigned_value"),
            References(),
        ),
        (
            "【NC_２】試す。",
            ("keyword_alias_nc",),
            ("necrocharge_threshold",),
            evidence(),
        ),
        (
            "【SC_２】試す。",
            ("keyword_alias_sc",),
            ("spell_chain_threshold",),
            evidence(),
        ),
    ],
)
def test_paired_caps_and_aliases_resolve_to_independent_roles_and_schema(
    text: str, identifiers: tuple[str, ...], roles: tuple[str, ...], refs: References
) -> None:
    item = entry(entry_ref(), partition(text)[0], VERSION)
    value = candidate(text, refs).model_copy(update={"inventory_id": item.id})
    assert recognize(text, partition(text)[0], value, refs) == ()
    part = partition(text)[0]
    classified, matches = classify(
        text, part, item, locate(text, (part,))[0], refs, identifiers
    )
    assert len(matches) == len(roles)
    member = _members(
        (item,),
        (classified,),
        {(item.source_ref.source_version_id, item.source_ref.locator): text},
    )[0]
    assert member.pending == ()
    assert member.roles == roles
    assert tuple(h.value for h in member.hints) == ((2, 5) if len(roles) == 2 else (2,))
    member.verify_schema(
        Schema(
            slots=tuple(
                Slot(
                    name=f"slot_{n}",
                    type="uint",
                    occurrences=(h.occurrence,),
                    min=0,
                    max=9007199254740991,
                    reference_kind=None,
                )
                for n, h in enumerate(member.hints)
            )
        )
    )
    assert candidate(text, refs).issues
    assert value.parameter_schema is None
    owned = value.model_copy(
        update={
            "slots": tuple(
                h.model_copy(update={"numeric_rule": "prefix_field_cost"})
                for h in value.slots
            )
        }
    )
    assert recognize(text, partition(text)[0], owned, refs, identifiers) == ()
    changed = value.model_copy(
        update={"slots": tuple(h.model_copy(update={"value": 7}) for h in value.slots)}
    )
    assert recognize(text, partition(text)[0], changed, refs, identifiers) == ()


@pytest.mark.parametrize(("identifier", "text", "role", "minimum"), CASES[2:])
def test_zero_multiplier_stays_unresolved_at_definition_projection(
    identifier: str, text: str, role: str, minimum: int
) -> None:
    del role
    assert minimum == 1
    text = text.replace("２", "０")
    item = entry(entry_ref(), partition(text)[0], VERSION)
    value = candidate(text).model_copy(update={"inventory_id": item.id})
    rows = recognize(text, partition(text)[0], value, References(), (identifier,))
    assert rows == ()
    part = partition(text)[0]
    classified, matches = classify(
        text, part, item, locate(text, (part,))[0], References(), (identifier,)
    )
    assert matches == ()
    member = _members(
        (item,),
        (classified,),
        {(item.source_ref.source_version_id, item.source_ref.locator): text},
    )[0]
    assert member.pending == ("numeric_role_requires_review",)
    assert member.hints[0].value == 0
    with pytest.raises(
        ValueError, match=r"^Template definition source has unresolved parameter roles$"
    ):
        member.verify_schema(
            Schema(
                slots=(
                    Slot(
                        name="slot_0",
                        type="uint",
                        occurrences=(value.slots[0].occurrence,),
                        reference_kind=None,
                        min=1,
                        max=9007199254740991,
                    ),
                )
            )
        )
