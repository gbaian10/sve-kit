"""Validate payment shapes, ordered author bytes and their precise context boundaries."""

import json
from copy import deepcopy
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import ValidationError

from sve_carddb.core.json import canonical, object_value, parse

from .author_cost_cases import CARD_COSTS, CARD_TARGETS, CARD_WRAPPERS, COSTS
from .author_parent_cases import PARENTS
from .test_author_schema_drift import NODES, assert_selector_definition

if TYPE_CHECKING:
    from pydantic import JsonValue

ADAPTERS = COSTS | PARENTS


def _validator(name: str) -> Draft202012Validator:
    return Draft202012Validator({"$defs": NODES["$defs"], "$ref": "#/$defs/" + name})


def _check(name: str, value: JsonValue, *, accepted: bool) -> None:
    assert _validator(name).is_valid(value) == accepted
    adapter = ADAPTERS[name]
    if accepted:
        decoded = adapter.validate_json(canonical(value))
        assert adapter.dump_python(decoded, mode="json") == value
    else:
        with pytest.raises(ValidationError):
            adapter.validate_json(json.dumps(value))


@pytest.mark.parametrize(
    "value", [v for _, v in CARD_COSTS], ids=[c for c, _ in CARD_COSTS]
)
def test_card_payment_fragments(value: JsonValue) -> None:
    _check("A.cost", value, accepted=True)
    assert not Draft202012Validator(NODES).is_valid(value)


@pytest.mark.parametrize(
    ("name", "value"),
    [(n, v) for _, n, v in CARD_WRAPPERS],
    ids=[c for c, _, _ in CARD_WRAPPERS],
)
def test_card_wrapper_fragments(name: str, value: JsonValue) -> None:
    _check(name, value, accepted=True)


@pytest.mark.parametrize(
    "value",
    [
        {},
        {"result": "paid_now"},
        {"pp": 1, "ep": 1},
        {"steps": []},
        {"steps": [{"pp": 1}]},
        {"steps": [{"pp": 1}, {"pp": 1}], "pp": 1},
        {"steps": [{"pp": 1, "ep": 1}, {"pp": 1}]},
        {"steps": [{"pp": 1, "result": "paid_now"}, {"pp": 1}]},
        {"steps": [{"pp": 1, "option": 1}, {"pp": 1}]},
        {"steps": [{"steps": [{"pp": 1}, {"pp": 1}]}, {"pp": 1}]},
        {"pp": 1, "result": "paid_now"},
        {"pp": 1, "option": 1},
        {"stack_counter_from": {"on": "this", "n": 1}},
        {"one_of": []},
        {"one_of": [{"pp": 1}]},
        {"one_of": [{"one_of": [{"pp": 1}, {"ep": 1}]}, {"pp": 1}]},
        {"one_of": [{"pp": 1, "result": "paid_now"}, {"ep": 1}]},
    ],
)
def test_cost_structure_is_closed(value: JsonValue) -> None:
    _check("A.cost", value, accepted=False)


@pytest.mark.parametrize(
    "value",
    [
        {"ep": 1},
        {"one_of": [{"steps": [{"pp": 1}, {"act_self": True}]}, {"ep": 1}]},
        {"steps": [{"one_of": [{"pp": 1}, {"ep": 1}]}, {"pp": 1}]},
    ],
)
def test_synthetic_composite_payments(value: JsonValue) -> None:
    _check("A.cost", value, accepted=True)


@pytest.mark.parametrize(
    "op", ["pp", "ep", "leader_life", "lesson", "earth_rite", "pp_max"]
)
@pytest.mark.parametrize("value", [-1, True, None, "1", {"param": "quantity"}])
def test_quantity_cost_rejects_foreign_domains(op: str, value: JsonValue) -> None:
    _check("A.cost", {op: value}, accepted=False)


@pytest.mark.parametrize("op", ["act_self", "self_to_cemetery", "banish_self"])
@pytest.mark.parametrize("value", [False, 1, None, {}, "true"])
def test_self_payment_is_strict_true(op: str, value: JsonValue) -> None:
    _check("A.cost", {op: value}, accepted=False)


@pytest.mark.parametrize(
    "mode",
    [
        {"n": 1},
        {"upto": "X"},
        {"any": True},
        {"groups": [{"sel": {"in": "this"}, "n": 1}, {"sel": {"who": "self"}, "n": 1}]},
    ],
)
@pytest.mark.parametrize(
    "op",
    [
        "discard",
        "banish",
        "act",
        "to_cemetery",
        "return_to_hand",
        "reveal",
        "flip_down",
        "flip_up",
        "to_ex",
        "to_deck_bottom",
        "to_deck",
    ],
)
def test_synthetic_material_modes(op: str, mode: dict[str, JsonValue]) -> None:
    value = dict(mode)
    if "groups" not in value:
        value["sel"] = {"who": "self", "zone": "hand"}
    value.update(result="materials", distinct_by="name", cost_sum_ge=1)
    if op == "to_deck":
        value["shuffle"] = True
    _check("A.cost", {op: value}, accepted=True)


@pytest.mark.parametrize(
    "value",
    [
        {"sel": {"who": "self"}},
        {"sel": {"leader": "self"}, "n": 1},
        {"sel": {"who": "self"}, "n": 1, "upto": 1},
        {"sel": {"who": "self"}, "any": True, "n": 1},
        {"sel": {"who": "self"}, "n": {"param": "amount"}},
        {"sel": {"who": "self"}, "n": 1, "cost_sum_le": 3},
        {"sel": {"who": "self"}, "n": 1, "include_self": True},
        {"groups": []},
        {"groups": [{"sel": {"in": "this"}, "n": 1}]},
        {
            "groups": [
                {"sel": {"in": "this"}, "n": 1, "result": "part"},
                {"sel": {"who": "self"}, "n": 1},
            ]
        },
        {
            "groups": [
                {"sel": {"in": "this"}, "n": 1},
                {"sel": {"who": "self"}, "n": 1},
            ],
            "sel": {"who": "self"},
        },
    ],
)
def test_material_rejects_context_and_mode_mixups(value: JsonValue) -> None:
    _check("A.cost.material", value, accepted=False)


@pytest.mark.parametrize(
    "extra",
    [
        {"upto": 1},
        {"any": True},
        {"distinct_by": "name"},
        {"cost_sum_ge": 1},
        {"ref": "this"},
        {"include_self": True},
        {"groups": []},
    ],
)
def test_random_discard_has_no_constrained_selection(
    extra: dict[str, JsonValue],
) -> None:
    _check(
        "A.cost",
        {
            "discard": {
                "sel": {"who": "self", "zone": "hand"},
                "n": 1,
                "random": True,
                **extra,
            }
        },
        accepted=False,
    )


def test_random_and_shuffle_flags_cannot_be_omitted_or_false() -> None:
    for flag in ({}, {"shuffle": False}):
        _check(
            "A.cost",
            {"to_deck": {"sel": {"who": "self"}, "n": 1, **flag}},
            accepted=False,
        )
    _check(
        "A.cost",
        {"discard": {"sel": {"who": "self"}, "n": 1, "random": False}},
        accepted=False,
    )
    _check(
        "A.cost",
        {"reveal": {"sel": {"who": "self"}, "n": 1, "random": True}},
        accepted=False,
    )


@pytest.mark.parametrize("name", ["A.cost.optional", "A.cost.additional"])
def test_wrapper_and_material_receipts_have_separate_positions(name: str) -> None:
    value: JsonValue = {
        "discard": {"sel": {"who": "self"}, "n": 1, "result": "material"},
        "result": "whole",
    }
    _check(name, value, accepted=True)
    _check("A.cost", value, accepted=False)
    _check(name, {"pp": 1, "option": 1}, accepted=name == "A.cost.optional")
    _check(name, {"pp": 1, "result": "this"}, accepted=False)


@pytest.mark.parametrize(
    "value",
    [
        [],
        [{"pp": 1, "option": 1}],
        [{"pp": 1}, {"ep": 1, "option": 2}],
        [{"pp": 1, "option": 0}, {"pp": 1, "option": 2}],
        [{"pp": 1, "option": True}, {"pp": 1, "option": 2}],
    ],
)
def test_multiple_optional_payments_require_mode_indices(value: JsonValue) -> None:
    _check("A.cost.optionals", value, accepted=False)


@pytest.mark.parametrize("key", ["optional_cost_delta", "optional_cost_set"])
def test_optional_modifiers_require_one_payment(key: str) -> None:
    _check("A.play.costs", {key: 2}, accepted=False)
    _check("A.play.costs", {key: 2, "optional_cost": {"pp": 1}}, accepted=True)
    _check(
        "A.play.costs",
        {key: 2, "optional_cost": [{"pp": 1, "option": 1}, {"pp": 1, "option": 2}]},
        accepted=False,
    )


def test_play_cost_fragment_is_not_a_complete_ability_header() -> None:
    _check("A.play.costs", {"cost_set": 1, "cost_delta": -2}, accepted=True)
    _check("A.play.costs", {"if": {"awakened": True}, "cost_delta": -2}, accepted=False)


@pytest.mark.parametrize(
    ("value", "accepted"),
    [
        ({"min": 0, "max": "X"}, True),
        ({"from_payment": "materials"}, True),
        ({"from_payment": "materials", "min": 0}, False),
        ({"from_payment": "this"}, False),
        ({"min": -1}, False),
        ({"max": {"param": "bound"}}, False),
    ],
)
def test_x_source_is_exclusive(value: JsonValue, *, accepted: bool) -> None:
    _check("A.x", value, accepted=accepted)


@pytest.mark.parametrize(
    ("value", "accepted"),
    [
        ({"if": {"awakened": True}, "cost": {"pp": 0}}, True),
        ({"if": {"awakened": True}, "cost": {"pp": 0}, "may": False}, True),
        ({"if": {"awakened": True}, "cost": {"pp": 0}, "may": True}, True),
        ({"cost": {"pp": 0}}, False),
        ({"if": {"awakened": True}, "cost_set": 0}, False),
        ({"if": {"awakened": True}, "cost": {"pp": 0}, "may": 1}, False),
    ],
)
def test_alternative_cost_is_not_a_cost_modifier(
    value: JsonValue, *, accepted: bool
) -> None:
    _check("A.cost.alt", value, accepted=accepted)


def _bytes(name: str, value: JsonValue) -> bytes:
    adapter = ADAPTERS[name]
    decoded = adapter.validate_json(canonical(value))
    return canonical(parse(adapter.dump_json(decoded)))


def test_decoded_cost_bytes_preserve_order_repetition_and_mapping_independence() -> (
    None
):
    first: JsonValue = {"discard": {"sel": {"zone": "hand", "who": "self"}, "n": 1}}
    reordered: JsonValue = {"discard": {"n": 1, "sel": {"who": "self", "zone": "hand"}}}
    assert _bytes("A.cost", first) == _bytes("A.cost", reordered)
    forward: JsonValue = {"steps": [first, {"pp": 1}, first]}
    assert parse(_bytes("A.cost", forward)) == forward
    assert _bytes("A.cost", forward) != _bytes(
        "A.cost", {"steps": [{"pp": 1}, first, first]}
    )
    assert _bytes("A.cost", forward) != _bytes("A.cost", {"steps": [first, {"pp": 1}]})


def test_decoded_targets_and_optional_payments_keep_array_order() -> None:
    first: JsonValue = {"bind": "first", "from": {"who": "self"}, "n": 1}
    second: JsonValue = {"bind": "second", "from": {"who": "opp"}, "n": 1}
    assert _bytes("A.targets", [first, second]) != _bytes("A.targets", [second, first])
    payments: JsonValue = [{"pp": 1, "option": 1}, {"pp": 1, "option": 2}]
    assert parse(_bytes("A.cost.optionals", payments)) == payments
    _check("A.targets", [], accepted=False)


@pytest.mark.parametrize("name", COSTS)
def test_new_node_drift_detects_changed_shape(name: str) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])[name])
    if "properties" in node:
        node["additionalProperties"] = True
    else:
        node["anyOf"] = []
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize(
    ("name", "key", "value"),
    [
        ("A.cost", "oneOf", []),
        ("A.cost.optional-mode", "required", []),
        ("A.play.costs", "dependentRequired", {}),
        ("A.play.costs", "then", {}),
        ("A.target.group-filter", "minProperties", 0),
        ("A.target.group-filter", "not", {}),
    ],
)
def test_drift_detects_semantic_constraints(
    name: str, key: str, value: JsonValue
) -> None:
    changed = deepcopy(NODES)
    object_value(object_value(changed["$defs"])[name])[key] = value
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


def test_cost_integer_rejects_float_at_both_boundaries() -> None:
    with pytest.raises(ValueError, match="Floating point"):
        parse(b'{"pp":1.0}')
    with pytest.raises(ValidationError):
        COSTS["A.cost"].validate_json('{"pp":1.0}')


@pytest.mark.parametrize(
    "value",
    [value for _, value in CARD_TARGETS],
    ids=[card for card, _ in CARD_TARGETS],
)
def test_card_grouped_and_distinct_targets(value: JsonValue) -> None:
    _check("A.target", value, accepted=True)


@pytest.mark.parametrize("mode", [{"n": 2}, {"upto": 2}, {"any": True}])
def test_grouped_target_synthetic_modes_match_one_branch(
    mode: dict[str, JsonValue],
) -> None:
    value: JsonValue = {
        "bind": "t",
        "from": {"who": "self"},
        "groups": [{"orig_cost_le": 5}, {"orig_cost_le": 3}],
        **mode,
    }
    _check("A.target", value, accepted=True)
    names = [
        "A.target." + suffix
        for suffix in [
            "exact",
            "up-to",
            "any",
            "grouped.exact",
            "grouped.up-to",
            "grouped.any",
        ]
    ]
    assert sum(_validator(name).is_valid(value) for name in names) == 1


@pytest.mark.parametrize(
    "group",
    [
        {},
        {"who": "self"},
        {"zone": "field"},
        {"in": "this"},
        {"except": {"who": "self"}},
        {"where": {"awakened": True}},
        {"as": "item_alias"},
        {"power_max": True},
        {"trait": {"param": "trait"}},
        {"kind": {"param": "kind"}},
        {"class": {"param": "class"}},
        {"name": {"param": "name"}},
        {"traits_all": [{"param": "trait"}]},
        {"token": True, "not_token": True},
        {"name": "name", "names": ["name"]},
        {"orig_cost_le": {"param": "bound"}},
    ],
)
def test_target_group_does_not_borrow_selector_permissions(group: JsonValue) -> None:
    _check("A.target.group-filter", group, accepted=False)
    _check(
        "A.target",
        {
            "bind": "t",
            "from": {"who": "self"},
            "groups": [group, {"orig_cost_le": 3}],
            "upto": 2,
        },
        accepted=False,
    )


def test_group_filters_keep_nested_quantity_operand_permissions() -> None:
    _check(
        "A.target.group-filter",
        {"orig_cost_le": {"minus": ["X", {"param": "offset"}]}},
        accepted=True,
    )
    _check(
        "A.target.group-filter",
        {"orig_cost_le": {"times": ["X", {"param": "offset"}]}},
        accepted=False,
    )


@pytest.mark.parametrize(
    "fields",
    [
        {"groups": [{"orig_cost_le": 3}]},
        {"from": {"leader": "opp"}},
        {"from": {"who": "opp", "leader_or": True}},
        {"distinct_by": "name", "distinct_names": True},
        {"roles": {}},
        {"option": True},
        {"upto": 1},
    ],
)
def test_grouped_target_rejects_wrong_context_and_exclusivity(
    fields: dict[str, JsonValue],
) -> None:
    _check(
        "A.target",
        {
            "bind": "t",
            "from": {"who": "self"},
            "groups": [{"orig_cost_le": 5}, {"orig_cost_le": 3}],
            "n": 2,
            **fields,
        },
        accepted=False,
    )


@pytest.mark.parametrize(
    ("name", "field", "constraint", "value"),
    [
        ("A.cost", "steps", "minItems", 1),
        ("A.cost.step", "one_of", "minItems", 1),
        ("A.cost.optional-mode", "option", "minimum", 0),
        ("A.cost.material.groups", "groups", "minItems", 1),
        ("A.target.grouped.exact", "groups", "minItems", 1),
    ],
)
def test_drift_detects_nested_bounds(
    name: str, field: str, constraint: str, value: JsonValue
) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])[name])
    object_value(object_value(node["properties"])[field])[constraint] = value
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize("name", ["A.cost.material", "A.cost.shuffle-material"])
@pytest.mark.parametrize(
    "mode",
    [
        {"n": 1},
        {"upto": 1},
        {"any": True},
        {"groups": [{"sel": {"in": "this"}, "n": 1}, {"sel": {"who": "self"}, "n": 1}]},
    ],
)
def test_material_union_has_exactly_one_branch(
    name: str, mode: dict[str, JsonValue]
) -> None:
    value = dict(mode)
    if "groups" not in value:
        value["sel"] = {"who": "self"}
    if name == "A.cost.shuffle-material":
        value["shuffle"] = True
    _check(name, value, accepted=True)
    assert (
        sum(
            _validator(name + "." + suffix).is_valid(value)
            for suffix in ["exact", "up-to", "any", "groups"]
        )
        == 1
    )


@pytest.mark.parametrize("zone", ["banish", "banished"])
def test_grouped_source_does_not_invent_a_banished_zone(zone: str) -> None:
    _check(
        "A.target",
        {
            "bind": "t",
            "from": {"who": "self", "zone": zone},
            "groups": [{"orig_cost_le": 4}, {"orig_cost_le": 3}, {"orig_cost_le": 2}],
            "upto": 3,
        },
        accepted=False,
    )


def test_drift_detects_group_filter_parameter_admission() -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])["A.target.group-filter"])
    object_value(node["properties"])["trait"] = {
        "anyOf": [{"type": "string"}, {"$ref": "#/$defs/A.param"}]
    }
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, "A.target.group-filter")
