"""Exercise recursive query roles independently from schema/type correspondence."""

import json
from itertools import combinations
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.core.json import canonical, object_value, parse

from .author_numeric_cases import NUMERIC, C, E
from .author_parent_cases import PARENTS
from .author_query_card_cases import CARD_QUERIES
from .author_receipt_cases import RECEIPTS
from .author_selector_cases import COUNTS, OBJECTS, SELECTORS

if TYPE_CHECKING:
    from pydantic import JsonValue

NODES = object_value(
    parse(
        (
            Path(__file__).resolve().parents[3] / "dsl" / "author-nodes.schema.json"
        ).read_bytes()
    )
)
ADAPTERS = NUMERIC | COUNTS | SELECTORS | PARENTS | RECEIPTS
COLLECTION = "A.collection"
COUNTED = "A.expr.count-source"
COMPARISONS = (
    "cost_eq",
    "cost_le",
    "cost_ge",
    "orig_cost_eq",
    "orig_cost_le",
    "orig_cost_ge",
    "hp_le",
)
FLAGS = (
    "not_same_name_as_this",
    "has_trigger",
    "has_evolve",
    "entered_this_turn",
    "advance_follower",
)
BOOLEANS = ("face_up", "boxed", "evolved")
ATTRIBUTES: dict[str, JsonValue] = {
    "not_class": "ロイヤル",
    "not_trait": "プリンセス",
    "traits_any": ["エルフ", "魔法使い"],
    "keyword": "ward",
    **dict.fromkeys(FLAGS, True),
    **dict.fromkeys(BOOLEANS, False),
    **dict.fromkeys(COMPARISONS, 2),
}


def _check(name: str, value: JsonValue, *, accepted: bool) -> None:
    adapter = ADAPTERS[name]
    if accepted:
        decoded = adapter.validate_json(canonical(value))
        assert adapter.dump_python(decoded, mode="json") == value
    else:
        with pytest.raises(ValidationError):
            adapter.validate_json(json.dumps(value))


@pytest.mark.parametrize("field", ATTRIBUTES)
@pytest.mark.parametrize("name", [OBJECTS, COUNTED, COLLECTION])
def test_new_filters_preserve_query_contexts(name: str, field: str) -> None:
    for source in ({"in": "shown"}, {"who": "self", "zone": "field"}):
        _check(name, {**source, field: ATTRIBUTES[field]}, accepted=True)
        _check(name, {**source, field: None}, accepted=False)
        _check(name, {**source, field: {"param": "value"}}, accepted=False)
    _check(
        "A.material-filter",
        {field: ATTRIBUTES[field]},
        accepted=field in {"orig_cost_eq", "orig_cost_ge", "not_class"},
    )


@pytest.mark.parametrize("field", COMPARISONS)
def test_filter_quantities_share_operations_but_not_signed_literals(field: str) -> None:
    value: JsonValue
    for name in (OBJECTS, COUNTED, COLLECTION):
        valid_quantities: tuple[JsonValue, ...] = (
            0,
            "X",
            {"minus": [0, 1]},
            {"plus": ["X", {"param": "offset"}]},
        )
        for value in valid_quantities:
            _check(name, {"in": "shown", field: value}, accepted=True)
        invalid_quantities: tuple[JsonValue, ...] = (-1, True, 1.0, {"param": "n"})
        for value in invalid_quantities:
            # JSON Schema integer cannot distinguish floats; the decoder must.
            if isinstance(value, float):
                with pytest.raises(ValidationError):
                    ADAPTERS[name].validate_json(
                        json.dumps({"in": "shown", field: value})
                    )
            else:
                _check(name, {"in": "shown", field: value}, accepted=False)


@pytest.mark.parametrize("field", FLAGS)
@pytest.mark.parametrize("value", [False, 1, 0, "true"])
def test_query_true_flags_are_strict(field: str, value: JsonValue) -> None:
    _check(COUNTED, {"in": "shown", field: value}, accepted=False)


@pytest.mark.parametrize("field", BOOLEANS)
def test_boolean_filter_false_is_distinct_from_absence(field: str) -> None:
    value: JsonValue
    for value in (True, False):
        _check(OBJECTS, {"in": "shown", field: value}, accepted=True)
    for value in (0, 1, "false"):
        _check(OBJECTS, {"in": "shown", field: value}, accepted=False)


def test_power_max_is_only_an_object_selector_filter() -> None:
    value: JsonValue
    query: dict[str, JsonValue] = {
        "in": "shown",
        "power_max": True,
        "except": {"in": "chosen"},
        "where": {"standing": "item"},
    }
    _check(OBJECTS, query, accepted=True)
    for name in (COUNTED, COLLECTION, "A.material-filter"):
        _check(name, query, accepted=False)
    invalid_flags: tuple[JsonValue, ...] = (False, 1, "true", {"param": "value"})
    for value in invalid_flags:
        _check(OBJECTS, {"in": "shown", "power_max": value}, accepted=False)


@pytest.mark.parametrize("field", ["who", "zone"])
def test_counted_source_parameters_do_not_leak_to_except_or_aggregates(
    field: str,
) -> None:
    source: dict[str, JsonValue] = {
        "who": "self",
        "zone": "field",
        field: {"param": "value"},
    }
    _check(COUNTED, source, accepted=True)
    _check(COUNTED, {"any_of": [source]}, accepted=True)
    _check(OBJECTS, {"in": "shown", "except": source}, accepted=True)
    _check(COLLECTION, source, accepted=False)
    for outer in ({"in": "shown"}, {"who": "self", "zone": "field"}):
        _check(COUNTED, {**outer, "except": source}, accepted=False)
    for key in ("distinct_names", "distinct_orig_costs"):
        _check(E, {key: source}, accepted=False)
    _check(E, {"agg": {"of": source, "attr": "cost", "fn": "sum"}}, accepted=False)
    nested: dict[str, JsonValue] = {
        "in": "shown",
        "except": {
            "in": "other_group",
            "as": "candidate",
            "where": {"cmp": {"left": {"count": source}, "op": "ge", "right": 1}},
        },
    }
    _check(COUNTED, nested, accepted=True)
    _check(E, {"distinct_names": nested}, accepted=True)


@pytest.mark.parametrize("name", [COUNTED, COLLECTION])
def test_collection_scopes_and_union_boundaries(name: str) -> None:
    value: JsonValue
    atom: dict[str, JsonValue] = {"in": "shown", "where": {"standing": "item"}}
    _check(name, {"any_of": [atom, {"who": "self", "zone": "field"}]}, accepted=True)
    invalid_queries: tuple[JsonValue, ...] = (
        {"in": "shown", "as": "candidate"},
        {"in": "shown", "as": "item", "where": {"standing": "item"}},
        {"in": "shown", "who": "self"},
        {"in": "shown", "zone": "field"},
        {"any_of": [atom], "where": {"my_turn": True}},
        {"any_of": [atom], "except": {"in": "chosen"}},
        {"any_of": [{"any_of": [atom]}]},
        {"any_of": []},
        {"in": "shown", "except": {"leader": "self"}},
    )
    for value in invalid_queries:
        _check(name, value, accepted=False)


@pytest.mark.parametrize("key", ["hand", "leader_life"])
def test_player_read_and_condition_payloads_are_disjoint(key: str) -> None:
    bad: JsonValue
    payload: JsonValue
    _check(E, {key: {"owner_of": "this"}}, accepted=True)
    _check(C, {key: {"owner_of": "this"}}, accepted=False)
    for op in ("eq", "le", "ge"):
        value: JsonValue = {
            key: {"who": "self", op: {"plus": [1, {"param": "offset"}]}}
        }
        _check(C, value, accepted=True)
        _check(E, value, accepted=False)
        invalid_thresholds: tuple[JsonValue, ...] = (-1, True, {"param": "n"})
        for bad in invalid_thresholds:
            _check(C, {key: {"who": "self", op: bad}}, accepted=False)
    for left, right in combinations(("eq", "le", "ge"), 2):
        _check(C, {key: {"who": "self", left: 1, right: 2}}, accepted=False)
    invalid_players: tuple[JsonValue, ...] = (
        {"eq": 1},
        {"who": "self"},
        {"who": "each", "eq": 1},
    )
    for payload in invalid_players:
        _check(C, {key: payload}, accepted=False)


def test_zone_free_thresholds_are_exact_or_at_least() -> None:
    payload: JsonValue
    for key in ("eq", "ge"):
        _check(C, {"zone_free": {"who": "self", "zone": "ex", key: 0}}, accepted=True)
    invalid_zones: tuple[JsonValue, ...] = (
        {"who": "self", "zone": "ex", "le": 1},
        {"who": "self", "zone": "deck", "ge": 1},
        {"who": "self", "zone": "ex", "eq": 0, "ge": 1},
        {"who": "self", "zone": {"param": "zone"}, "ge": 1},
    )
    for payload in invalid_zones:
        _check(C, {"zone_free": payload}, accepted=False)


@pytest.mark.parametrize("key", ["my_turn", "opp_turn", "awakened", "crimson"])
@pytest.mark.parametrize("value", [False, 1, 0, "true"])
def test_condition_flags_are_true_only(key: str, value: JsonValue) -> None:
    _check(C, {key: value}, accepted=False)


@pytest.mark.parametrize("key", ["combo", "spellchain", "necro"])
def test_rule_thresholds_are_positive_constants(key: str) -> None:
    value: JsonValue
    for value in (1, 9007199254740991):
        _check(C, {key: value}, accepted=True)
    invalid_constants: tuple[JsonValue, ...] = (
        0,
        -1,
        True,
        9007199254740992,
        "X",
        {"param": "n"},
        {"plus": [1, 2]},
    )
    for value in invalid_constants:
        _check(C, {key: value}, accepted=False)


@pytest.mark.parametrize("attr", ["cost", "orig_cost", "power"])
@pytest.mark.parametrize("fn", ["sum", "min", "max"])
def test_aggregate_attribute_function_matrix(attr: str, fn: str) -> None:
    _check(E, {"agg": {"of": {"in": "shown"}, "attr": attr, "fn": fn}}, accepted=True)


@pytest.mark.parametrize(
    "value",
    [
        {"agg": {"of": {"in": "shown"}, "attr": "unknown", "fn": "sum"}},
        {"agg": {"of": {"in": "shown"}, "attr": "cost", "fn": "mean"}},
        {"entered_from": {"of": "this", "from": "hand"}},
        {"played_from": {"ref": "this", "zone": "ex"}},
        {"entered_by": {"ref": "this", "by": "effect"}},
        {"in_zone": {"ref": "this", "zone": ["field", "ex"]}},
        {"construction": {"who": "self", "title": "コラボ"}},
        {"construction": ""},
        {"construction": {"param": "title"}},
        {"counter": {"on": {"who": "self"}, "name": "start"}},
        {"counter": {"on": "this", "name": ""}},
    ],
)
def test_new_payloads_reject_wrong_types_and_unregistered_spellings(
    value: JsonValue,
) -> None:
    _check(E, value, accepted=False)
    _check(C, value, accepted=False)


@pytest.mark.parametrize(
    "key",
    [
        "history",
        "history_count",
        "history_sum",
        "event",
        "lki_of",
        "at",
        "chosen",
        "paid",
        "x_paid",
        "dice",
    ],
)
def test_empty_history_and_capture_payloads_stay_rejected(key: str) -> None:
    for name in (E, C):
        _check(name, {key: "this"}, accepted=False)


@pytest.mark.parametrize("field", ["not_class", "not_trait", "keyword"])
@pytest.mark.parametrize("value", ["", 1, True, [], {"param": "value"}])
def test_new_text_filters_are_nonempty_literals(field: str, value: JsonValue) -> None:
    _check(OBJECTS, {"in": "shown", field: value}, accepted=False)


@pytest.mark.parametrize("value", [[], [""], [True], [{"param": "trait"}], "機械"])
def test_traits_any_has_nonempty_literal_members(value: JsonValue) -> None:
    _check(COUNTED, {"in": "shown", "traits_any": value}, accepted=False)


def test_snapshot_queries_and_material_filters_keep_explicit_views() -> None:
    source: dict[str, JsonValue] = {
        "in": {"objects_of": "moved", "view": "before"},
        "not_class": "ロイヤル",
    }
    _check(E, {"count": source}, accepted=True)
    _check(C, {"exists": source}, accepted=True)
    _check(
        C,
        {
            "did": {
                "result": "moved",
                "where": {"not_class": "ロイヤル"},
                "view": "after",
            }
        },
        accepted=True,
    )
    _check(
        E,
        {
            "count_of": {
                "receipt": "moved",
                "where": {"not_class": "ロイヤル"},
                "view": "before",
            }
        },
        accepted=True,
    )


def test_attribute_conjunctions_may_describe_empty_sets() -> None:
    _check(
        OBJECTS,
        {"in": "shown", "class": "ロイヤル", "not_class": "ロイヤル"},
        accepted=True,
    )
    _check(
        COUNTED, {"in": "shown", "trait": "機械", "not_trait": "機械"}, accepted=True
    )


@pytest.mark.parametrize(
    ("name", "value"),
    [(name, value) for _, name, value in CARD_QUERIES],
    ids=[card for card, _, _ in CARD_QUERIES],
)
def test_card_query_fragments(name: str, value: JsonValue) -> None:
    _check(name, value, accepted=True)
