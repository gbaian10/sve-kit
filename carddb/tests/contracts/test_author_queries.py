"""Exercise recursive query roles independently from schema/type correspondence."""

import json
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


@pytest.mark.parametrize(
    ("name", "value"),
    [(name, value) for _, name, value in CARD_QUERIES],
    ids=[card for card, _, _ in CARD_QUERIES],
)
def test_card_query_fragments(name: str, value: JsonValue) -> None:
    _check(name, value, accepted=True)
