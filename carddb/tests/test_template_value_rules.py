"""Assignment, comparison and sum roles require their own closed grammatical contexts."""

import pytest

from sve_carddb.template_parameters.explicit_rules import EXPLICIT

from .test_template_explicit_rules import CASES as FIRST_CASES
from .test_template_explicit_rules import (
    test_current_resolution_carries_role_and_rejects_weakened_numeric_bounds as check_resolution,
)
from .test_template_explicit_rules import (
    test_explicit_match_requires_opt_in_exact_value_and_preserves_proposal as check_match,
)
from .test_template_rule_candidates import matches

CASES = (
    ("cost_assignment", "それのコストを２にする。", "cost_assigned_value", 0),
    (
        "original_cost_bound",
        "それの元のコストが２以上なら試す。",
        "original_cost_bound",
        0,
    ),
    (
        "original_cost_sum_bound",
        "元のコストの合計が２以下になるように試す。",
        "original_cost_sum_bound",
        0,
    ),
    ("pp_capacity_bound", "自分のPP最大値が２なら試す。", "pp_capacity_bound", 0),
    ("pp_remaining_bound", "自分の残りPPが２以上なら試す。", "pp_remaining_bound", 0),
    ("attack_assignment", "それの攻撃力を２にする。", "attack_assigned_value", 0),
    ("health_assignment", "これの体力を２にする。", "health_assigned_value", 0),
    (
        "attack_health_assignment",
        "それの攻撃力と体力を２にする。",
        "attack_health_assigned_value",
        0,
    ),
    (
        "leader_health_assignment",
        "自分のリーダーの体力を２にする。",
        "leader_health_assigned_value",
        0,
    ),
    (
        "leader_health_bound",
        "自分のリーダーの体力が２以下なら試す。",
        "leader_health_bound",
        0,
    ),
    ("attack_bound", "これの攻撃力が２以上である限り試す。", "attack_bound", 0),
    (
        "attack_sum_bound",
        "場の試験体の攻撃力の合計が２以上なら試す。",
        "attack_sum_bound",
        0,
    ),
)


@pytest.mark.parametrize(("identifier", "text", "role", "minimum"), CASES)
def test_value_match_requires_opt_in_raw_value_and_preserves_candidate(
    identifier: str, text: str, role: str, minimum: int
) -> None:
    check_match(identifier, text, role, minimum)
    assert matches(text.replace("２", "０"), identifier)


@pytest.mark.parametrize(("identifier", "text", "role", "minimum"), CASES)
def test_value_resolution_checks_exact_role_schema_and_original_ownership(
    identifier: str, text: str, role: str, minimum: int
) -> None:
    check_resolution(identifier, text, role, minimum)


@pytest.mark.parametrize(("identifier", "text", "role", "minimum"), CASES)
def test_unrecognized_following_word_does_not_inherit_value_role(
    identifier: str, text: str, role: str, minimum: int
) -> None:
    del role, minimum
    assert matches(text.replace("２", "２猫"), identifier) == ()


@pytest.mark.parametrize(
    ("identifier", "text"),
    [
        ("cost_assignment", "それの元のコストを２にする。"),
        ("cost_assignment", "超コストを２にする。"),
        ("cost_assignment", "コストが２にする。"),
        ("cost_assignment", "コストを２にする名。"),
        ("cost_assignment", "コストを２にして読む。"),
        ("original_cost_bound", "元のコストが２なら試す。"),
        ("original_cost_bound", "それのコストが２なら試す。"),
        ("original_cost_bound", "それの元のコストが２個なら試す。"),
        ("original_cost_sum_bound", "コストの合計が２以下なら試す。"),
        ("original_cost_sum_bound", "元のコストが２以下になるように試す。"),
        ("original_cost_sum_bound", "元のコストの合計が２だけなら試す。"),
        ("pp_capacity_bound", "自分のPPが２なら試す。"),
        ("pp_capacity_bound", "自分の残りPPが２なら試す。"),
        ("pp_capacity_bound", "自分のPP最大値を２なら試す。"),
        ("pp_remaining_bound", "自分のPP最大値が２なら試す。"),
        ("pp_remaining_bound", "自分の残りEPが２なら試す。"),
        ("pp_remaining_bound", "残りPPが２なら試す。"),
        ("attack_assignment", "それの体力を２にする。"),
        ("attack_assignment", "それの攻撃力が２にする。"),
        ("attack_assignment", "それの攻撃力を２にする名。"),
        ("health_assignment", "これの攻撃力を２にする。"),
        ("health_assignment", "自分のリーダーの体力を２にする。"),
        ("attack_health_assignment", "それの攻撃力や体力を２にする。"),
        ("attack_health_assignment", "それの攻撃力と体力が２にする。"),
        ("leader_health_assignment", "それの体力を２にする。"),
        ("leader_health_assignment", "自分のリーダーの体力が２にする。"),
        ("leader_health_bound", "これの体力が２以下なら試す。"),
        ("leader_health_bound", "自分のリーダーの体力２以下なら試す。"),
        ("leader_health_bound", "自分のリーダーの体力が２枚なら試す。"),
        ("attack_bound", "これの攻撃力の合計が２なら試す。"),
        ("attack_bound", "これの体力が２なら試す。"),
        ("attack_bound", "攻撃力が２なら試す。"),
        ("attack_sum_bound", "場の試験体の攻撃力が２以上なら試す。"),
        ("attack_sum_bound", "場の試験体の体力の合計が２以上なら試す。"),
    ],
)
def test_wrong_stat_resource_particle_and_continuation_stay_pending(
    identifier: str, text: str
) -> None:
    assert matches(text, identifier) == ()
    assert matches("『" + text + "』", identifier) == ()


def test_observed_symbolic_health_and_evolution_cost_shapes_are_explicit() -> None:
    for text in (
        "自分のリーダーの{体力}２以下なら試す。",
        "自分のリーダーの{体力}が２以下である限り試す。",
        "自分のリーダーの{体力}２以下になったとき試す。",
    ):
        assert matches(text, "leader_health_bound")
    assert matches(
        "相手のリーダーすべての{体力}を２にする。", "leader_health_assignment"
    )
    assert matches("これの{進化}コストを２にする。", "cost_assignment")
    assert matches("それのコストを２にしてプレイする。", "cost_assignment")
    assert matches("自分と相手のPP最大値が２なら試す。", "pp_capacity_bound")
    assert {c[0] for c in FIRST_CASES + CASES} <= set(EXPLICIT)
