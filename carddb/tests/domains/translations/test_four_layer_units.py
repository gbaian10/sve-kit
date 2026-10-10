"""Fixed source-unit cases exercise the actual construction-specific unit registry."""

from pathlib import Path

import pytest
from pydantic import JsonValue, TypeAdapter

from sve_carddb.core.json import array, object_value, string
from sve_carddb.domains.translations.four_layer_units import (
    CountContext,
    count_context,
    source_unit,
)

_DOCUMENT = object_value(
    TypeAdapter(JsonValue).validate_json(
        (
            Path(__file__).resolve().parents[4]
            / "docs/schema/domains/four-layer-cases.json"
        ).read_bytes()
    )
)
_CASES = tuple(
    object_value(c)
    for c in array(_DOCUMENT["cases"])
    if object_value(c)["operation"] == "source_unit"
)
_CONSTRUCTIONS = {
    "fixture.select_card": "select.card.v1",
    "fixture.select_union": "select.union.v1",
    "fixture.select_leader": "select.leader.v1",
}


@pytest.mark.parametrize("case", _CASES, ids=lambda c: string(c["id"]))
def test_fixed_source_units(case: dict[str, JsonValue]) -> None:
    inputs, expected = object_value(case["input"]), object_value(case["expected"])
    token = inputs["token"]
    assert isinstance(token, bool)
    context = CountContext(
        _CONSTRUCTIONS[string(inputs["constructor"])],
        string(inputs["counted_object"]),
        tuple(string(z) for z in array(inputs["counted_zones"])),
        token,
        string(inputs["quantity_role"]),
    )
    result = source_unit(context, string(inputs["raw_unit"]))
    assert result.merge_allowed == expected["merge_allowed"]
    assert result.reason == expected["reason"]


@pytest.mark.parametrize(
    ("constructor", "zones", "token", "role"),
    [
        ("unrecognized", ("hand",), False, "cardinality"),
        ("select.card.v1", ("ex",), False, "cardinality"),
        ("select.card.v1", ("battlefield", "ex"), False, "cardinality"),
        ("select.card.v1", ("hand",), False, "repeat_count"),
        ("select.union.v1", ("hand",), False, "cardinality"),
    ],
)
def test_nearby_contexts_do_not_borrow_a_unit_rule(
    constructor: str, zones: tuple[str, ...], token: bool, role: str
) -> None:
    context = CountContext(constructor, "follower", zones, token, role)
    result = source_unit(context, "枚")
    assert not result.merge_allowed
    assert result.reason == "source_unit_rule_unresolved"


def test_wrong_classifier_is_retained_as_an_exact_source_diagnostic() -> None:
    context = CountContext(
        "select.card.v1", "follower", ("battlefield",), False, "cardinality"
    )
    assert source_unit(context, "体").merge_allowed
    for raw in ("枚", "回", "體", " 体"):
        result = source_unit(context, raw)
        assert not result.merge_allowed
        assert result.reason == "source_unit_mismatch"


def test_counted_zone_context_does_not_silently_repair_input() -> None:
    with pytest.raises(ValueError, match="ordered and unique"):
        CountContext(
            "select.union.v1",
            "follower",
            ("ex", "battlefield"),
            False,
            "union_cardinality",
        )
    with pytest.raises(ValueError, match="ordered and unique"):
        CountContext(
            "select.union.v1", "follower", ("ex", "ex"), False, "union_cardinality"
        )


@pytest.mark.parametrize(
    "before",
    [
        "自分のEXエリアのフォロワー",
        "自分の場か自分のEXエリアの仮族・フォロワー",
        "自分の手札の元のコストN以下の仮族・フォロワー",
    ],
)
def test_unrestricted_source_nps_include_both_token_states(before: str) -> None:
    found = count_context(before)
    assert found is not None
    assert found.token == frozenset({True, False})
    assert source_unit(found, "枚").merge_allowed


def test_incomplete_prefix_cannot_borrow_a_counted_set_suffix() -> None:
    assert count_context("不明な自分の場のフォロワー") is None
    assert count_context("自分場のフォロワー") is None


def test_explicit_token_filter_is_preserved_instead_of_becoming_unrestricted() -> None:
    found = count_context("自分の場のトークン・フォロワー")
    assert found is not None
    assert found.token is True
    assert source_unit(found, "体").merge_allowed
    assert not source_unit(found, "枚").merge_allowed


def test_a_trait_prefix_cannot_swallow_another_counted_np_and_its_zone() -> None:
    found = count_context(
        "相手の場のフォロワーN体と自分の墓場の元のコストNの仮族・フォロワー"
    )
    assert found is not None
    assert found.counted_zones == ("graveyard",)
    assert source_unit(found, "枚").merge_allowed


def test_an_explicit_two_np_union_preserves_both_zones() -> None:
    found = count_context("進化前の自分の場のフォロワーや自分のEXエリアのフォロワー")
    assert found is not None
    assert found.counted_zones == ("battlefield", "ex")
    assert found.quantity_role == "union_cardinality"
    assert source_unit(found, "枚").merge_allowed
    assert not source_unit(found, "体").merge_allowed
