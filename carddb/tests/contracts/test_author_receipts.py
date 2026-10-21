"""Receipt reads preserve projection, value categories and closed material fields."""

import json
from copy import deepcopy
from itertools import combinations
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import TypeAdapter, ValidationError

from sve_carddb.contracts import author_references as refs
from sve_carddb.core.json import canonical, object_value, parse

from .author_numeric_cases import NUMERIC, C, E
from .author_receipt_cases import (
    CONFIG,
    COUNTED,
    DID,
    RECEIPT_CASES,
    RECEIPTS,
    SNAPSHOT,
)

if TYPE_CHECKING:
    from pydantic import JsonValue

SCHEMA_PATH = Path(__file__).resolve().parents[3] / "dsl" / "author-nodes.schema.json"
ADAPTERS = (
    RECEIPTS
    | NUMERIC
    | {
        "A.ref": TypeAdapter(refs.Ref, config=CONFIG),
        "A.player-ref": TypeAdapter(refs.PlayerRef, config=CONFIG),
        "A.leader-recipient": TypeAdapter(refs.LeaderRecipient, config=CONFIG),
    }
)
ATTRIBUTES: dict[str, JsonValue] = {
    "name": "ブラスター・ダーク",
    "zone": "field",
    "orig_cost_eq": 2,
}
MATERIALS = [
    {key: ATTRIBUTES[key] for key in keys}
    for size in range(1, len(ATTRIBUTES) + 1)
    for keys in combinations(ATTRIBUTES, size)
]


@pytest.fixture(scope="module")
def schema() -> dict[str, JsonValue]:
    result = object_value(parse(SCHEMA_PATH.read_bytes()))
    Draft202012Validator.check_schema(result)
    return result


def _check(
    schema: dict[str, JsonValue],
    name: str,
    value: JsonValue,
    *,
    accepted: bool,
) -> None:
    validator = Draft202012Validator(
        {"$id": schema["$id"], "$defs": schema["$defs"], "$ref": "#/$defs/" + name}
    )
    assert validator.is_valid(value) == accepted
    adapter = ADAPTERS[name]
    if accepted:
        decoded = adapter.validate_json(canonical(value), strict=True)
        assert adapter.dump_python(decoded, mode="json") == value
    else:
        with pytest.raises(ValidationError):
            adapter.validate_json(json.dumps(value), strict=True)


@pytest.mark.parametrize(("name", "value"), RECEIPT_CASES)
def test_receipt_nodes_round_trip(
    schema: dict[str, JsonValue], name: str, value: JsonValue
) -> None:
    _check(schema, name, value, accepted=True)
    assert not Draft202012Validator(schema).is_valid(value)


def test_receipt_examples_cover_every_definition() -> None:
    assert {name for name, _ in RECEIPT_CASES} == RECEIPTS.keys()


@pytest.mark.parametrize("where", MATERIALS)
def test_material_attributes_are_optional_but_conjunctive(
    schema: dict[str, JsonValue], where: dict[str, JsonValue]
) -> None:
    _check(schema, "A.material-filter", where, accepted=True)
    for view in ("before", "after"):
        _check(
            schema,
            E,
            {"count_of": {"receipt": "revealed", "where": where, "view": view}},
            accepted=True,
        )
    _check(schema, C, {"did": {"result": "paid_cost", "where": where}}, accepted=True)


@pytest.mark.parametrize(
    "where",
    [
        {},
        None,
        [],
        True,
        {"name": ""},
        {"name": None},
        {"name": {"param": "name"}},
        {"zone": ["field", "ex"]},
        {"zone": {"param": "zone"}},
        {"zone": "banish"},
        {"orig_cost_eq": -1},
        {"orig_cost_eq": 9007199254740992},
        {"orig_cost_eq": True},
        {"orig_cost_eq": "2"},
        {"orig_cost_eq": {"param": "cost"}},
        {"who": "self"},
        {"zone": "field", "who": "self"},
        {"in": "shown"},
        {"follow": "moved"},
        {"where": {"did": "paid_cost"}},
        {"as": "material"},
        {"actual": 1},
        {"evolved": True},
        {"view": "before"},
    ],
)
def test_material_filter_does_not_select_live_sources(
    schema: dict[str, JsonValue], where: JsonValue
) -> None:
    _check(schema, "A.material-filter", where, accepted=False)
    _check(schema, E, {"count_of": {**COUNTED, "where": where}}, accepted=False)
    _check(schema, C, {"did": {**DID, "where": where}}, accepted=False)


@pytest.mark.parametrize(
    "where",
    [
        b'{"orig_cost_eq":true}',
        b'{"orig_cost_eq":"2"}',
        b'{"zone":"field","who":"self"}',
    ],
    ids=["bool", "numeric-string", "unknown-beside-known"],
)
def test_material_filter_is_strict_without_caller_strict_mode(where: bytes) -> None:
    # _check passes strict=True, which would hide a lax TypedDict configuration.
    with pytest.raises(ValidationError):
        RECEIPTS["A.material-filter"].validate_json(where)
    counted = b'{"count_of":{"receipt":"shown","where":' + where + b',"view":"before"}}'
    with pytest.raises(ValidationError):
        ADAPTERS[E].validate_json(counted)


@pytest.mark.parametrize(
    "receipt", ["revealed", {"event": "move_receipt"}, {"entry_payment": "additional"}]
)
def test_receipt_sources_do_not_infer_binding_authority(
    schema: dict[str, JsonValue], receipt: JsonValue
) -> None:
    _check(schema, E, {"count_of": receipt}, accepted=True)
    _check(schema, C, {"did": receipt}, accepted=True)
    _check(schema, E, {"count_of": {**COUNTED, "receipt": receipt}}, accepted=True)
    _check(schema, C, {"did": {**DID, "result": receipt}}, accepted=True)
    for view in ("before", "after"):
        _check(
            schema,
            "A.snapshot-read",
            {"objects_of": receipt, "view": view},
            accepted=True,
        )


@pytest.mark.parametrize(
    "receipt",
    [
        "this",
        "self",
        "opponents",
        "",
        "result\n",
        {"event": "holder"},
        {"follow": "moved"},
        {"param": "receipt"},
        {"actual": 1},
        {"entry_payment": "optional"},
        {"result": "paid_cost"},
        SNAPSHOT,
    ],
)
def test_receipt_sources_reject_other_reference_categories(
    schema: dict[str, JsonValue], receipt: JsonValue
) -> None:
    _check(schema, E, {"count_of": receipt}, accepted=False)
    _check(schema, C, {"did": receipt}, accepted=False)
    _check(
        schema,
        "A.snapshot-read",
        {"objects_of": receipt, "view": "before"},
        accepted=False,
    )


@pytest.mark.parametrize(
    "value",
    [SNAPSHOT, {"same_as": SNAPSHOT}, {"lki_of": "this"}],
)
@pytest.mark.parametrize("name", [E, C, "A.ref", "A.player-ref", "A.leader-recipient"])
def test_snapshots_do_not_grant_numeric_or_operation_authority(
    schema: dict[str, JsonValue], value: JsonValue, name: str
) -> None:
    _check(schema, name, value, accepted=False)


@pytest.mark.parametrize(
    "value",
    [
        "this",
        {"follow": "moved"},
        {"lki_of": "this"},
        {"param": "name"},
        {"objects_of": "material"},
        {"who": "self", "zone": "field"},
    ],
)
def test_same_as_requires_an_explicit_material_projection(
    schema: dict[str, JsonValue], value: JsonValue
) -> None:
    _check(schema, "A.same-as", {"same_as": value}, accepted=False)


@pytest.mark.parametrize("view", [None, True, 1, "live", "Before", {"param": "view"}])
def test_views_are_closed(schema: dict[str, JsonValue], view: JsonValue) -> None:
    _check(schema, "A.snapshot-read", {**SNAPSHOT, "view": view}, accepted=False)
    _check(schema, E, {"count_of": {**COUNTED, "view": view}}, accepted=False)


@pytest.mark.parametrize(("name", "value"), RECEIPT_CASES)
def test_receipt_discriminators_and_required_fields(
    schema: dict[str, JsonValue], name: str, value: dict[str, JsonValue]
) -> None:
    for key in value:
        changed = deepcopy(value)
        changed.pop(key)
        _check(schema, name, changed, accepted=False)
        changed = deepcopy(value)
        changed["wrong_key"] = changed.pop(key)
        _check(schema, name, changed, accepted=False)
        changed = deepcopy(value)
        changed[key] = None
        _check(schema, name, changed, accepted=False)
    _check(schema, name, value | {"follow": "moved"}, accepted=False)


@pytest.mark.parametrize(
    "value",
    [
        {"count_of": {**COUNTED, "result": "revealed"}},
        {"count_of": {**COUNTED, "event": "move_receipt"}},
        {"did": {**DID, "receipt": "paid_cost"}},
        {"did": {"actual": 1}},
        {"did": {"evolved": True}},
        {"did": {"payment_complete": True}},
        {"count_of": "result", "did": "result"},
    ],
)
def test_receipt_variants_cannot_be_mixed(
    schema: dict[str, JsonValue], value: JsonValue
) -> None:
    _check(schema, E, value, accepted=False)
    _check(schema, C, value, accepted=False)


def test_documented_receipts_use_the_existing_recursive_operators(
    schema: dict[str, JsonValue],
) -> None:
    _check(schema, E, {"times": [{"count_of": "returned"}, 2]}, accepted=True)
    _check(schema, E, {"count_of": COUNTED}, accepted=True)
    _check(schema, "A.same-as", {"same_as": SNAPSHOT}, accepted=True)
    _check(schema, C, {"not": {"did": "evolved_now"}}, accepted=True)
    _check(schema, C, {"did": DID}, accepted=True)
    _check(
        schema,
        C,
        {"cmp": {"left": {"count_of": COUNTED}, "op": "eq", "right": 2}},
        accepted=True,
    )
    _check(schema, E, {"plus": [SNAPSHOT, 1]}, accepted=False)
    _check(schema, E, {"plus": [{"did": "paid_cost"}, 1]}, accepted=False)


def _leaves(
    node: dict[str, JsonValue], schema: dict[str, JsonValue]
) -> list[dict[str, JsonValue]]:
    ref = node.get("$ref")
    if isinstance(ref, str):
        return _leaves(
            object_value(object_value(schema["$defs"])[ref.removeprefix("#/$defs/")]),
            schema,
        )
    variants = node.get("anyOf", node.get("oneOf"))
    if isinstance(variants, list):
        return [
            leaf
            for branch in variants
            for leaf in _leaves(object_value(branch), schema)
        ]
    return [node]


@pytest.mark.parametrize(
    ("name", "key", "values"),
    [
        (
            "A.expr.count-of",
            "count_of",
            [
                "receipt",
                {"event": "move_receipt"},
                {"entry_payment": "additional"},
                COUNTED,
            ],
        ),
        (
            "A.cond.did",
            "did",
            [
                "receipt",
                {"event": "move_receipt"},
                {"entry_payment": "additional"},
                DID,
            ],
        ),
        (E, None, [{"count_of": "receipt"}, {"count_of": COUNTED}]),
        (C, None, [{"did": "receipt"}, {"did": DID}]),
        ("A.snapshot-read", None, [SNAPSHOT]),
    ],
)
def test_receipt_unions_have_unique_leaves_and_exactly_one_match(
    schema: dict[str, JsonValue],
    name: str,
    key: str | None,
    values: list[JsonValue],
) -> None:
    node = object_value(object_value(schema["$defs"])[name])
    if key is not None:
        node = object_value(object_value(node["properties"])[key])
    leaves = _leaves(node, schema)
    assert len({canonical(leaf) for leaf in leaves}) == len(leaves)
    for value in values:
        assert (
            sum(
                Draft202012Validator(
                    {"$id": schema["$id"], "$defs": schema["$defs"], **leaf}
                ).is_valid(value)
                for leaf in leaves
            )
            == 1
        )


@pytest.mark.parametrize(
    ("name", "raw"),
    [
        ("A.material-filter", b'{"orig_cost_eq":2.0}'),
        (
            E,
            b'{"count_of":{"receipt":"shown","where":{"orig_cost_eq":2.0},"view":"before"}}',
        ),
        (C, b'{"did":{"result":"paid_cost","where":{"orig_cost_eq":2.0}}}'),
    ],
    ids=["material", "count", "did"],
)
def test_material_numbers_reject_floats_at_both_strict_boundaries(
    name: str, raw: bytes
) -> None:
    with pytest.raises(ValueError, match="Floating point"):
        parse(raw)
    with pytest.raises(ValidationError):
        ADAPTERS[name].validate_json(raw, strict=True)
