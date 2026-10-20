"""Quantity modes and their parents retain distinct source and parameter domains."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import TypeAdapter, ValidationError

from sve_carddb.contracts.author_references import Ref
from sve_carddb.core.json import canonical, object_value, parse

from .author_numeric_cases import EXPRESSIONS, NUMERIC
from .author_parent_cases import CARD_PARENTS, DECK_CASES, PARENTS
from .author_recipient_cases import RECIPIENTS
from .author_selector_cases import CONFIG, SELECTORS

if TYPE_CHECKING:
    from pydantic import JsonValue

NODES = object_value(
    parse(
        (
            Path(__file__).resolve().parents[3] / "dsl" / "author-nodes.schema.json"
        ).read_bytes()
    )
)
ADAPTERS = (
    PARENTS
    | NUMERIC
    | SELECTORS
    | RECIPIENTS
    | {"A.ref": TypeAdapter(Ref, config=CONFIG)}
)
BASES: dict[str, dict[str, JsonValue]] = {
    "A.target": {"bind": "t", "from": {"in": "looked"}},
    "A.act.choose": {"bind": "picked", "from": {"in": "looked"}},
    "A.act.search": {"sel": {"who": "self", "zone": "deck"}, "to": "hand"},
    "A.cost.counter.self": {"on": "this", "name": "童話"},
    "A.cost.counter.selected": {
        "on": {"who": "each", "zone": "field", "name": "竜山温泉"},
        "name": "神湯",
        "holders": 1,
    },
}


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


def _branches(name: str) -> list[str]:
    node = object_value(object_value(NODES["$defs"])[name])
    variants = node.get("anyOf")
    if not isinstance(variants, list):
        return [name]
    result = []
    for branch in variants:
        reference = object_value(branch)["$ref"]
        assert isinstance(reference, str)
        result.extend(_branches(reference.removeprefix("#/$defs/")))
    return result


@pytest.mark.parametrize(
    ("name", "value"),
    [(name, value) for _, name, value in CARD_PARENTS],
    ids=[card for card, _, _ in CARD_PARENTS],
)
def test_card_parent_fragments(name: str, value: JsonValue) -> None:
    _check(name, value, accepted=True)
    root = name.rsplit(".", 1)[0]
    _check(root, value, accepted=True)
    if root.startswith("A.cost.counter"):
        _check("A.cost.counter", value, accepted=True)
    assert not Draft202012Validator(NODES).is_valid(value)


@pytest.mark.parametrize(
    "value", [value for _, value in DECK_CASES], ids=[card for card, _ in DECK_CASES]
)
def test_card_deck_fragments(value: JsonValue) -> None:
    _check("A.deck-slice", value, accepted=True)


@pytest.mark.parametrize("name", BASES)
@pytest.mark.parametrize("key", ["n", "upto", "any"])
def test_every_parent_mode_matches_one_branch(name: str, key: str) -> None:
    value = {**BASES[name], key: True if key == "any" else 1}
    accepted = not (name == "A.act.search" and key == "any")
    _check(name, value, accepted=accepted)
    branches = _branches(name)
    assert len(branches) == len(set(branches))
    assert sum(_validator(branch).is_valid(value) for branch in branches) == accepted


@pytest.mark.parametrize("name", BASES)
@pytest.mark.parametrize(
    "quantity",
    [
        {},
        {"n": 1, "upto": 1},
        {"n": 1, "any": True},
        {"upto": 1, "any": True},
        {"n": 1, "upto": 1, "any": True},
    ],
)
def test_quantity_modes_are_required_and_exclusive(
    name: str, quantity: dict[str, JsonValue]
) -> None:
    _check(name, BASES[name] | quantity, accepted=False)


@pytest.mark.parametrize("name", BASES)
@pytest.mark.parametrize("key", ["n", "upto"])
@pytest.mark.parametrize(
    "quantity", [-1, True, False, "1", None, [], {}, 9007199254740992]
)
def test_parent_quantity_rejects_wrong_domains(
    name: str, key: str, quantity: JsonValue
) -> None:
    _check(name, {**BASES[name], key: quantity}, accepted=False)


@pytest.mark.parametrize("name", BASES)
@pytest.mark.parametrize("quantity", [0, 1, 9007199254740991, "X", {"minus": [0, 2]}])
def test_quantity_does_not_clamp_signed_operations(
    name: str, quantity: JsonValue
) -> None:
    _check(name, {**BASES[name], "n": quantity}, accepted=True)


@pytest.mark.parametrize("name", BASES)
@pytest.mark.parametrize("key", ["n", "upto"])
def test_selection_parameters_stay_in_whitelisted_parent_positions(
    name: str, key: str
) -> None:
    _check(
        name,
        {**BASES[name], key: {"param": "amount"}},
        accepted=not name.startswith("A.cost"),
    )


@pytest.mark.parametrize("name", ["A.quantity", "A.selection-quantity"])
@pytest.mark.parametrize("value", [value for _, value in EXPRESSIONS])
def test_all_existing_operations_share_the_quantity_core(
    name: str, value: JsonValue
) -> None:
    _check(name, value, accepted=True)


@pytest.mark.parametrize("name", BASES)
@pytest.mark.parametrize(
    ("value", "accepted"),
    [
        ({"times": [2, {"minus": ["X", {"param": "offset"}]}]}, True),
        ({"half_up": {"plus": [{"param": "offset"}, "X"]}}, True),
        ({"times": [2, {"param": "offset"}]}, False),
        ({"minus": [{"param": "offset"}, "X"]}, False),
        ({"plus": [{"param": "a"}, {"param": "b"}]}, False),
        ({"minus": [True, 1]}, False),
        ({"times": [1, "2"]}, False),
    ],
)
def test_nested_arithmetic_uses_operand_permissions(
    name: str, value: JsonValue, *, accepted: bool
) -> None:
    _check(name, {**BASES[name], "n": value}, accepted=accepted)


@pytest.mark.parametrize("name", BASES)
@pytest.mark.parametrize("value", [1, 0, False, "true", [], None, {"param": "all"}])
def test_any_is_strict_true(name: str, value: JsonValue) -> None:
    _check(name, {**BASES[name], "any": value}, accepted=False)


@pytest.mark.parametrize("who", ["self", "opp", "each", "opponents", "other_players"])
def test_deck_slice_preserves_player_cardinality(who: str) -> None:
    _check("A.deck-slice", {"who": who, "n": 2}, accepted=True)


@pytest.mark.parametrize(
    "value",
    [
        {"who": "self"},
        {"n": 1},
        {"who": {"param": "player"}, "n": 1},
        {"who": "self", "n": {"param": "amount"}},
        {"who": "self", "n": -1},
        {"who": "self", "upto": 1},
        {"who": "self", "any": True},
        {"who": "self", "n": 1, "zone": "deck"},
        {"who": "self", "n": 1, "kind": "follower"},
    ],
)
def test_deck_slice_is_a_closed_position(value: JsonValue) -> None:
    _check("A.deck-slice", value, accepted=False)


@pytest.mark.parametrize("key", ["top", "bottom"])
@pytest.mark.parametrize("name", ["A.sel.object-core", "A.sel.recipient", "A.ref"])
def test_slices_do_not_enter_unregistered_selector_positions(
    key: str, name: str
) -> None:
    _check(name, {key: {"who": "self", "n": 1}}, accepted=False)
    _check(name, {"any_of": [{key: {"who": "self", "n": 1}}]}, accepted=False)


@pytest.mark.parametrize("name", BASES)
@pytest.mark.parametrize(
    "extra",
    [
        {"unknown": True},
        {"groups": []},
        {"if": {"did": "paid_now"}},
        {"position": "bottom"},
        {"entry_effects": []},
        {"precondition": True},
        {"instead": []},
    ],
)
def test_parent_rejects_unimplemented_or_foreign_fields(
    name: str, extra: dict[str, JsonValue]
) -> None:
    _check(name, {**BASES[name], "n": 1, **extra}, accepted=False)


@pytest.mark.parametrize("name", ["A.target", "A.act.choose"])
@pytest.mark.parametrize("bind", [None, "this", "1", "", {"param": "bind"}])
def test_bind_is_a_declaration_name(name: str, bind: JsonValue) -> None:
    _check(name, {**BASES[name], "n": 1, "bind": bind}, accepted=False)


@pytest.mark.parametrize(
    ("name", "fields"),
    [
        ("A.target", {"option": 1, "distinct_by": "class"}),
        (
            "A.act.choose",
            {
                "by": {"master_of": "t"},
                "may": False,
                "reveal": True,
                "order": False,
                "distinct_by": "orig_cost",
            },
        ),
        ("A.act.search", {"result": "searched"}),
        ("A.cost.counter.selected", {"result": "paid_counter"}),
    ],
)
def test_optional_parent_fields_preserve_authored_values(
    name: str, fields: dict[str, JsonValue]
) -> None:
    _check(name, {**BASES[name], "n": 1, **fields}, accepted=True)


@pytest.mark.parametrize("option", [0, -1, True, None, "1", {"param": "mode"}])
def test_target_option_is_a_positive_mode_index(option: JsonValue) -> None:
    _check("A.target", {**BASES["A.target"], "n": 1, "option": option}, accepted=False)


@pytest.mark.parametrize("field", ["may", "reveal", "order"])
@pytest.mark.parametrize("value", [None, 0, 1, "true"])
def test_choose_boolean_fields_are_strict(field: str, value: JsonValue) -> None:
    _check(
        "A.act.choose",
        {**BASES["A.act.choose"], "n": 1, field: value},
        accepted=False,
    )


def test_deck_slice_keeps_legal_nested_offset_parameters() -> None:
    _check(
        "A.deck-slice",
        {"who": "self", "n": {"half_up": {"minus": ["X", {"param": "offset"}]}}},
        accepted=True,
    )
    _check(
        "A.deck-slice",
        {"who": "self", "n": {"times": ["X", {"param": "offset"}]}},
        accepted=False,
    )


@pytest.mark.parametrize("name", BASES)
def test_parent_required_fields(name: str) -> None:
    for key in BASES[name]:
        value = {**BASES[name], "n": 1}
        value.pop(key)
        _check(name, value, accepted=False)


@pytest.mark.parametrize("name", ["A.act.choose", "A.act.search", "A.cost.counter"])
def test_object_parents_reject_leader_sources(name: str) -> None:
    value: dict[str, JsonValue]
    if name == "A.cost.counter":
        value = {**BASES["A.cost.counter.selected"], "n": 1, "on": {"leader": "opp"}}
    else:
        value = {
            **BASES[name],
            "n": 1,
            "from" if name == "A.act.choose" else "sel": {"leader": "opp"},
        }
    _check(name, value, accepted=False)


def test_roles_belong_only_to_targets() -> None:
    roles: JsonValue = {"primary": {"if_nonempty": 1}, "rest": "remaining"}
    for name in ("A.target", "A.act.choose"):
        _check(
            name,
            {**BASES[name], "n": 1, "roles": roles},
            accepted=name == "A.target",
        )
    _check("A.target", {**BASES["A.target"], "n": 1, "roles": {}}, accepted=False)


@pytest.mark.parametrize("key", ["n", "upto", "any"])
def test_target_distinct_spellings_are_exclusive_and_preserved(key: str) -> None:
    base = {**BASES["A.target"], key: True if key == "any" else 1}
    first = {**base, "distinct_names": True}
    second = {**base, "distinct_by": "name"}
    for value in (first, second):
        _check("A.target", value, accepted=True)
    assert canonical(first) != canonical(second)
    _check("A.target", {**first, "distinct_by": "name"}, accepted=False)
    _check("A.target", {**first, "distinct_by": "orig_cost"}, accepted=False)


@pytest.mark.parametrize("name", ["A.target", "A.act.choose", "A.act.search"])
@pytest.mark.parametrize("value", [-1, {"param": "limit"}, None])
def test_cost_sum_bound_does_not_borrow_selection_parameters(
    name: str, value: JsonValue
) -> None:
    _check(name, {**BASES[name], "n": 1, "cost_sum_le": value}, accepted=False)


@pytest.mark.parametrize("value", [-1, None, True, {"param": "holders"}])
def test_counter_holders_have_their_own_quantity_domain(value: JsonValue) -> None:
    _check(
        "A.cost.counter",
        {**BASES["A.cost.counter.selected"], "n": 2, "holders": value},
        accepted=False,
    )


def test_counter_this_does_not_ask_for_a_holder() -> None:
    _check(
        "A.cost.counter",
        {**BASES["A.cost.counter.self"], "n": 2, "holders": 1},
        accepted=False,
    )
    _check(
        "A.cost.counter",
        {
            **BASES["A.cost.counter.selected"],
            "n": 2,
            "holders": {"minus": ["X", {"param": "offset"}]},
        },
        accepted=True,
    )
    _check(
        "A.cost.counter",
        {**BASES["A.cost.counter.self"], "n": 2, "name": {"param": "counter"}},
        accepted=False,
    )


def test_search_destination_parameters_do_not_replace_structure() -> None:
    _check(
        "A.act.search",
        {**BASES["A.act.search"], "n": 1, "to": {"param": "destination"}},
        accepted=True,
    )
    _check(
        "A.act.search",
        {
            **BASES["A.act.search"],
            "n": 1,
            "to": {"order": "printed", "partition": [{"zone": "hand", "n": 1}]},
        },
        accepted=False,
    )


@pytest.mark.parametrize("name", ["A.quantity", "A.selection-quantity"])
@pytest.mark.parametrize("value", [0, "X", *[value for _, value in EXPRESSIONS]])
def test_quantity_variants_have_no_overlap(name: str, value: JsonValue) -> None:
    branches = _branches(name)
    assert len(branches) == len(set(branches))
    assert sum(_validator(branch).is_valid(value) for branch in branches) == 1


@pytest.mark.parametrize("value", [-1, 0, 1, True, None])
def test_numeric_operation_excludes_literals(value: JsonValue) -> None:
    _check("A.expr.numeric-operation", value, accepted=False)


@pytest.mark.parametrize(
    ("name", "value", "accepted"),
    [
        ("CounterName", "神湯", True),
        ("CounterName", "", False),
        ("CounterName", {"param": "name"}, False),
        ("DistinctBy", "orig_cost", True),
        ("DistinctBy", "class", True),
        ("DistinctBy", "name", True),
        ("DistinctBy", "power", False),
    ],
)
def test_parent_leaf_domains(name: str, value: JsonValue, *, accepted: bool) -> None:
    _check(name, value, accepted=accepted)


@pytest.mark.parametrize("name", BASES)
def test_parent_float_rejection_at_strict_boundary(name: str) -> None:
    raw = json.dumps({**BASES[name], "n": 1.0})
    with pytest.raises(ValueError, match="Floating point"):
        parse(raw.encode())
    with pytest.raises(ValidationError):
        ADAPTERS[name].validate_json(raw)
