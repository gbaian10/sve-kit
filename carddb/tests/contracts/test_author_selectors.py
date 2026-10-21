"""Closed query variants preserve contexts, optional fields and recursion."""

import json
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
    "value",
    [{}, {"zone": "field"}],
)
def test_object_sources_reject_mixed_variants_and_contexts(value: JsonValue) -> None:
    _check(OBJECTS, value, accepted=False)


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
