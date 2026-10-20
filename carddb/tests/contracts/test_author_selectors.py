"""Closed query variants preserve contexts, optional fields and recursion."""

import json
from copy import deepcopy
from itertools import combinations
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import TypeAdapter, ValidationError

from sve_carddb.contracts.author_references import Ref
from sve_carddb.core.json import canonical, object_value, parse

from .author_numeric_cases import NUMERIC, C, E
from .author_selector_cases import (
    CONFIG,
    COUNT_UNION,
    COUNTED,
    COUNTS,
    OBJECTS,
    QUERY_CASES,
    SELECTORS,
)

if TYPE_CHECKING:
    from pydantic import JsonValue

NODES = object_value(
    parse(
        (
            Path(__file__).resolve().parents[3] / "dsl" / "author-nodes.schema.json"
        ).read_bytes()
    )
)
ADAPTERS = COUNTS | SELECTORS | NUMERIC | {"A.ref": TypeAdapter(Ref, config=CONFIG)}
FILTERS: dict[str, JsonValue] = {
    "kind": "follower",
    "class": "ロイヤル",
    "trait": "機械",
    "traits_all": ["機械", "錬金術師"],
    "name": "アマテラス",
    "names": ["アマテラス", "ツクヨミ"],
    "name_contains": "セッカ",
    "other": True,
    "orig_cost_eq": 0,
    "orig_cost_le": 3,
    "orig_cost_ge": 1,
    "hp_le": 2,
    "acted": False,
    "racing": True,
    "token": True,
    "not_token": True,
}


def _check(name: str, value: JsonValue, *, accepted: bool) -> None:
    validator = Draft202012Validator(
        {"$defs": NODES["$defs"], "$ref": "#/$defs/" + name}
    )
    assert validator.is_valid(value) == accepted
    # TypedDict configurations must enforce strictness without caller overrides.
    if accepted:
        decoded = ADAPTERS[name].validate_json(canonical(value))
        assert ADAPTERS[name].dump_python(decoded, mode="json") == value
    else:
        with pytest.raises(ValidationError):
            ADAPTERS[name].validate_json(json.dumps(value))


@pytest.mark.parametrize(("name", "value"), QUERY_CASES)
def test_query_variants_round_trip(name: str, value: JsonValue) -> None:
    _check(name, value, accepted=True)
    assert not Draft202012Validator(NODES).is_valid(value)


def test_query_cases_cover_every_definition() -> None:
    assert {name for name, _ in QUERY_CASES} == (COUNTS | SELECTORS).keys()


@pytest.mark.parametrize("field", FILTERS)
@pytest.mark.parametrize("source", ["zone", "in", "count"])
def test_each_filter_in_object_positions(field: str, source: str) -> None:
    body: dict[str, JsonValue] = (
        {"in": "shown"} if source == "in" else {"who": "self", "zone": "field"}
    )
    body[field] = FILTERS[field]
    _check(COUNTED if source == "count" else OBJECTS, body, accepted=True)
    body[field] = None
    _check(COUNTED if source == "count" else OBJECTS, body, accepted=False)


@pytest.mark.parametrize("field", ["kind", "class", "trait", "name"])
def test_filter_parameters_use_only_whitelisted_leaves(field: str) -> None:
    body: dict[str, JsonValue] = {"who": "self", "zone": "field"}
    body[field] = {"param": "value"}
    _check(OBJECTS, body, accepted=True)
    _check(COUNTED, body, accepted=False)
    body = {**body, "traits_all": [{"param": "trait"}, "機械"]}
    _check(OBJECTS, body, accepted=True)


@pytest.mark.parametrize("field", ["kind", "class", "trait", "traits_all", "name"])
def test_count_attributes_do_not_inherit_selector_parameter_permissions(
    field: str,
) -> None:
    parameter: JsonValue = {"param": "value"}
    attribute: JsonValue = [parameter] if field == "traits_all" else parameter
    query: dict[str, JsonValue] = {
        "who": "self",
        "zone": "field",
        field: attribute,
    }
    _check(COUNTED, query, accepted=False)
    _check(COUNTED, {"any_of": [query]}, accepted=False)
    _check(E, {"count": query}, accepted=False)
    condition: dict[str, JsonValue] = {
        "cmp": {"left": {"count": query}, "op": "ge", "right": 1}
    }
    _check(OBJECTS, {"in": "shown", "where": condition}, accepted=False)
    _check(OBJECTS, query, accepted=True)


@pytest.mark.parametrize("field", ["kind", "name"])
def test_same_as_is_snapshot_comparison(field: str) -> None:
    body: dict[str, JsonValue] = {
        "in": "shown",
        field: {"same_as": {"objects_of": "discarded", "view": "before"}},
    }
    _check(OBJECTS, body, accepted=True)
    _check(
        COUNTED,
        {"who": "self", "zone": "field", field: body[field]},
        accepted=True,
    )
    body[field] = {"same_as": "this"}
    _check(OBJECTS, body, accepted=False)


@pytest.mark.parametrize(
    "who",
    [
        "self",
        "each",
        "opponents",
        "other_players",
        {"param": "owner"},
        {"owner_of": "this"},
    ],
)
@pytest.mark.parametrize("zone", ["field", ["field", "ex"], {"param": "zone"}])
def test_sources_accept_scalars_sets_and_leaf_parameters(
    who: JsonValue, zone: JsonValue
) -> None:
    value = {"who": who, "zone": zone}
    _check(OBJECTS, value, accepted=True)
    _check(COUNTED, value, accepted=True)


@pytest.mark.parametrize(
    "value",
    [
        {},
        {"zone": "field"},
        {"who": "self", "in": "shown"},
        {"in": "shown", "zone": "field"},
        {"who": None},
        {"who": ["self", "opp"]},
        {"who": "self", "zone": []},
        {"who": "self", "zone": [None]},
        {"who": "self", "zone": [{"param": "zone"}]},
        {"in": {"entry_payment": "additional"}},
        {"in": {"count_of": "moved"}},
        {"in": {"objects_of": "moved"}},
        {"in": {"leader": "self"}},
        {"in": ["shown"]},
        {"any_of": [{"in": "shown"}]},
        {"top": {"who": "self", "n": 1}},
        {"bottom": {"who": "self", "n": 1}},
        {"leader": "self"},
        {"who": "self", "leader_or": True, "kind": "follower"},
        {"rest": "shown"},
        {"who": "self", "unknown": True},
        {"who": "self", "except": "chosen"},
        {"who": "self", "except": {"leader": "opp"}},
        {"who": "self", "except": {"any_of": [{"in": "shown"}]}},
        {"who": "self", "as": "candidate"},
        {"who": "self", "as": "item", "where": {"did": "moved"}},
        {"who": "self", "where": {"count": {"who": "self", "zone": "field"}}},
        {"who": "self", "where": {}},
        {"who": "self", "where": None},
        {"who": "self", "evolved": False},
        {"who": "self", "cost_lt": 3},
        {"who": "self", "names": []},
        {"who": "self", "names": [{"param": "name"}]},
        {"who": "self", "traits_all": {"param": "traits"}},
        {"who": "self", "traits_all": []},
        {"who": "self", "orig_cost_ge": {"param": "cost"}},
        {"who": "self", "hp_le": {"bound": "limit"}},
        {"who": "self", "name_contains": ""},
    ],
)
def test_object_sources_reject_mixed_variants_and_contexts(value: JsonValue) -> None:
    _check(OBJECTS, value, accepted=False)


@pytest.mark.parametrize(
    ("left", "right"),
    [("token", "not_token"), *combinations(("name", "names", "name_contains"), 2)],
)
def test_exclusive_filters(left: str, right: str) -> None:
    value: dict[str, JsonValue] = {
        "who": "self",
        "zone": "field",
        left: FILTERS[left],
        right: FILTERS[right],
    }
    _check(OBJECTS, value, accepted=False)
    _check(COUNTED, value, accepted=False)
    _check(COUNTED, {"any_of": [value]}, accepted=False)


@pytest.mark.parametrize("field", ["other", "token", "not_token"])
@pytest.mark.parametrize("value", [False, 1, 0, "true", None])
def test_true_flags_are_strict(field: str, value: JsonValue) -> None:
    _check(OBJECTS, {"who": "self", field: value}, accepted=False)
    _check(COUNTED, {"who": "self", "zone": "field", field: value}, accepted=False)


@pytest.mark.parametrize("field", ["acted", "racing"])
@pytest.mark.parametrize("value", [1, 0, "false", None])
def test_boolean_attributes_are_strict(field: str, value: JsonValue) -> None:
    _check(OBJECTS, {"in": "shown", field: value}, accepted=False)


@pytest.mark.parametrize(
    "field", ["orig_cost_eq", "orig_cost_le", "orig_cost_ge", "hp_le"]
)
@pytest.mark.parametrize("value", [-1, 9007199254740992, True, "2", "X"])
def test_comparison_constants_are_not_selection_quantities(
    field: str, value: JsonValue
) -> None:
    _check(OBJECTS, {"in": "shown", field: value}, accepted=False)


@pytest.mark.parametrize(
    "value",
    [
        {"who": "self"},
        {"in": "shown"},
        {"who": "self", "zone": "field", "except": {"in": "shown"}},
        {"who": "self", "zone": "field", "where": {"did": "moved"}},
        {"who": "self", "zone": "field", "as": "candidate"},
        {"any_of": []},
        {"any_of": [{"who": "self"}]},
        {"any_of": [{"in": "shown"}]},
        {"any_of": [COUNT_UNION]},
        {"any_of": [{"who": "self", "zone": "field"}], "who": "self"},
        {"any_of": [{"who": "self", "zone": "field"}], "zone": "field"},
        {"any_of": [{"leader": "self"}]},
    ],
)
def test_count_source_is_a_separate_closed_query(value: JsonValue) -> None:
    _check(COUNTED, value, accepted=False)
    _check(E, {"count": value}, accepted=False)


def test_count_and_conditions_recurse_through_existing_numeric_types() -> None:
    count: dict[str, JsonValue] = {"count": COUNT_UNION}
    _check(E, count, accepted=True)
    _check(E, {"plus": [1, count]}, accepted=True)
    condition: dict[str, JsonValue] = {
        "all": [{"did": "moved"}, {"cmp": {"left": count, "op": "ge", "right": 8}}]
    }
    _check(C, condition, accepted=True)
    _check(
        OBJECTS, {"in": "shown", "as": "candidate", "where": condition}, accepted=True
    )
    _check(OBJECTS, {"in": "shown", "where": condition}, accepted=True)
    for name in (C, OBJECTS, "A.ref"):
        _check(name, count, accepted=False)
    for name in (E, C, "A.ref"):
        _check(name, {"who": "self", "zone": "field"}, accepted=False)


def test_except_keeps_the_object_domain_at_each_depth() -> None:
    value: dict[str, JsonValue] = {"in": "looked"}
    for _ in range(4):
        value = {"in": "looked", "except": value}
    _check(OBJECTS, value, accepted=True)
    changed = deepcopy(value)
    changed["except"] = {"leader": "self"}
    _check(OBJECTS, changed, accepted=False)


def _leaves(node: dict[str, JsonValue]) -> list[dict[str, JsonValue]]:
    reference = node.get("$ref")
    if isinstance(reference, str):
        return _leaves(
            object_value(
                object_value(NODES["$defs"])[reference.removeprefix("#/$defs/")]
            )
        )
    branches = node.get("anyOf", node.get("oneOf"))
    if isinstance(branches, list):
        return [leaf for branch in branches for leaf in _leaves(object_value(branch))]
    return [node]


@pytest.mark.parametrize(
    ("name", "values"),
    [
        (
            OBJECTS,
            [
                {"who": "self"},
                {"in": "shown"},
                {"in": "looked", "except": {"in": "chosen"}},
            ],
        ),
        (COUNTED, [{"who": "self", "zone": "field"}, COUNT_UNION]),
        (E, [{"count": COUNT_UNION}, {"count": {"who": "each", "zone": "field"}}]),
    ],
)
def test_query_unions_have_unique_leaves_and_one_match(
    name: str, values: list[JsonValue]
) -> None:
    leaves = _leaves(object_value(object_value(NODES["$defs"])[name]))
    assert len({canonical(leaf) for leaf in leaves}) == len(leaves)
    for value in values:
        assert (
            sum(
                Draft202012Validator({"$defs": NODES["$defs"], **leaf}).is_valid(value)
                for leaf in leaves
            )
            == 1
        )


@pytest.mark.parametrize(
    ("name", "payload"),
    [
        (OBJECTS, b'{"who":"self","acted":1}'),
        (OBJECTS, b'{"in":"shown","hp_le":2.0}'),
        (OBJECTS, b'{"in":"shown","class_":"royal"}'),
        (OBJECTS, b'{"in":"shown","as_":"candidate"}'),
        (OBJECTS, b'{"in":"shown","except_":{"in":"chosen"}}'),
        (COUNTED, b'{"who":"self","zone":"field","orig_cost_eq":2.0}'),
        (COUNTED, b'{"who":"self","zone":"field","token":1}'),
        (COUNTED, b'{"who":"self","zone":"field","unknown":true}'),
    ],
    ids=[
        "bool",
        "float",
        "class-alias",
        "scope-alias",
        "except-alias",
        "count-float",
        "count-flag",
        "count-extra",
    ],
)
def test_adapter_enforces_configuration_without_caller_strict(
    name: str, payload: bytes
) -> None:
    with pytest.raises(ValidationError):
        ADAPTERS[name].validate_json(payload)
