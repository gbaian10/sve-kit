"""Numeric fragment behavior, including placement, recursive shape and wire keys."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import ValidationError

from sve_carddb.core.json import canonical, object_value, parse

from .author_numeric_cases import CMP, CONDITIONS, EXPRESSIONS, NUMERIC, PAYLOADS, C, E

if TYPE_CHECKING:
    from pydantic import JsonValue

SCHEMA_PATH = Path(__file__).resolve().parents[3] / "dsl" / "author-nodes.schema.json"
SAFE = 9007199254740991


@pytest.fixture(scope="module")
def schema() -> dict[str, JsonValue]:
    result = object_value(parse(SCHEMA_PATH.read_bytes()))
    Draft202012Validator.check_schema(result)
    return result


def _validator(schema: dict[str, JsonValue], name: str) -> Draft202012Validator:
    return Draft202012Validator(
        {"$id": schema["$id"], "$defs": schema["$defs"], "$ref": "#/$defs/" + name}
    )


def _accept(schema: dict[str, JsonValue], name: str, value: JsonValue) -> None:
    assert _validator(schema, name).is_valid(value)
    adapter = NUMERIC[name]
    decoded = adapter.validate_json(canonical(value), strict=True)
    assert adapter.dump_python(decoded, mode="json") == value


def _reject(schema: dict[str, JsonValue], name: str, value: JsonValue) -> None:
    assert not _validator(schema, name).is_valid(value)
    # Out-of-range integers must reach the decoder independently of canonical().
    with pytest.raises(ValidationError):
        NUMERIC[name].validate_json(json.dumps(value), strict=True)


@pytest.mark.parametrize(("name", "value"), EXPRESSIONS)
def test_expression_variants(
    schema: dict[str, JsonValue], name: str, value: dict[str, JsonValue]
) -> None:
    _accept(schema, name, value)
    _accept(schema, E, value)
    _accept(schema, "A.expr.numeric-operation", value)
    _reject(schema, C, value)


@pytest.mark.parametrize(("name", "value"), CONDITIONS)
def test_condition_variants(
    schema: dict[str, JsonValue], name: str, value: dict[str, JsonValue]
) -> None:
    _accept(schema, name, value)
    _accept(schema, C, value)
    _reject(schema, E, value)


@pytest.mark.parametrize(("name", "value"), PAYLOADS)
def test_payloads_are_not_standalone_expressions(
    schema: dict[str, JsonValue], name: str, value: dict[str, JsonValue]
) -> None:
    _accept(schema, name, value)
    _reject(schema, E, value)
    _reject(schema, C, value)


@pytest.mark.parametrize("value", [-SAFE, -1, 0, 1, SAFE, "X", "lowest"])
def test_numeric_scalar_values(schema: dict[str, JsonValue], value: JsonValue) -> None:
    _accept(schema, E, value)
    _reject(schema, C, value)


@pytest.mark.parametrize(
    "value",
    [
        True,
        False,
        None,
        "1",
        "-1",
        "this",
        "self",
        "opponents",
        "X\n",
        "event.amount",
        -9007199254740992,
        9007199254740992,
        [],
        {},
        {"param": "n"},
        {"unknown": 1},
    ],
)
def test_invalid_scalars_and_unregistered_nodes(
    schema: dict[str, JsonValue], value: JsonValue
) -> None:
    _reject(schema, E, value)
    _reject(schema, C, value)


@pytest.mark.parametrize("key", ["plus", "minus", "times"])
@pytest.mark.parametrize("operands", [[], [1], [1, 2, 3]])
def test_binary_arity(
    schema: dict[str, JsonValue], key: str, operands: list[JsonValue]
) -> None:
    _reject(schema, "A.expr." + key, {key: operands})
    _reject(schema, E, {key: operands})


@pytest.mark.parametrize(
    ("name", "key", "root"),
    [
        ("A.expr.min", "min", E),
        ("A.expr.max", "max", E),
        ("A.cond.all", "all", C),
        ("A.cond.any", "any", C),
    ],
)
@pytest.mark.parametrize("payload", [[], True, 1])
def test_nonempty_combinations(
    schema: dict[str, JsonValue], name: str, key: str, root: str, payload: JsonValue
) -> None:
    _reject(schema, name, {key: payload})
    _reject(schema, root, {key: payload})


@pytest.mark.parametrize("unit", [1, SAFE])
def test_positive_unit_limits(schema: dict[str, JsonValue], unit: int) -> None:
    _accept(schema, E, {"per": {"of": -SAFE, "unit": unit}})


@pytest.mark.parametrize("unit", [0, -1, SAFE + 1, True, "1", {"param": "unit"}])
def test_invalid_units(schema: dict[str, JsonValue], unit: JsonValue) -> None:
    value: JsonValue = {"per": {"of": 5, "unit": unit}}
    _reject(schema, "A.expr.per", value)
    _reject(schema, E, value)


@pytest.mark.parametrize(
    "value",
    [
        {"zone_free": {"who": "self", "zone": "deck"}},
        {"zone_free": {"who": "self", "zone": ["field", "ex"]}},
        {"resource": {"who": "opponents", "kind": "pp"}},
        {"resource": {"who": "self", "kind": "life"}},
        {"turn_number": {"who": "each"}},
        {"power_of": {"leader": "self"}},
        {"hp_of": {"who": "opp", "zone": "field"}},
        {"bound": {"follow": "moved"}},
        {"cost_of": "opponents"},
    ],
)
def test_read_contexts(schema: dict[str, JsonValue], value: JsonValue) -> None:
    _reject(schema, E, value)
    _reject(schema, C, value)


@pytest.mark.parametrize("kind", ["pp_max", "pp", "ep", "sep"])
@pytest.mark.parametrize(
    "who",
    [
        "self",
        "opp",
        "turn",
        "non_turn",
        "current",
        "item",
        "player_alias",
        {"master_of": "chosen_object"},
        {"owner_of": {"event": "holder"}},
    ],
)
def test_player_reads(schema: dict[str, JsonValue], kind: str, who: JsonValue) -> None:
    _accept(schema, E, {"resource": {"who": who, "kind": kind}})
    _accept(schema, E, {"turn_number": {"who": who}})
    for zone in ("field", "ex"):
        _accept(schema, E, {"zone_free": {"who": who, "zone": zone}})


@pytest.mark.parametrize("key", ["power_of", "hp_of", "cost_of"])
@pytest.mark.parametrize(
    "ref",
    [
        "this",
        "item",
        "found",
        "chosen_object",
        {"follow": {"event": "move_receipt"}},
        {"role": {"of": "chosen_group", "name": "primary"}},
        {"event": "holder"},
    ],
)
def test_object_reads(schema: dict[str, JsonValue], key: str, ref: JsonValue) -> None:
    _accept(schema, E, {key: ref})


@pytest.mark.parametrize(
    "value",
    [
        {"plus": [{"param": "a"}, {"param": "b"}]},
        {"minus": [{"param": "a"}, 1]},
        {"minus": [{"param": "a"}, {"param": "b"}]},
        {"times": [{"param": "a"}, 1]},
        {"times": [1, {"param": "a"}]},
        {"half_up": {"param": "a"}},
        {"per": {"of": {"param": "a"}, "unit": 2}},
        {"per": {"of": 2, "unit": {"param": "a"}}},
        {"min": [{"param": "a"}]},
        {"max": [{"param": "a"}]},
        {"bound": {"param": "a"}},
        {"power_of": {"param": "a"}},
        {"hp_of": {"param": "a"}},
        {"cost_of": {"param": "a"}},
        {"resource": {"who": {"param": "a"}, "kind": "pp"}},
        {"resource": {"who": "self", "kind": {"param": "a"}}},
        {"turn_number": {"who": {"param": "a"}}},
        {"zone_free": {"who": {"param": "a"}, "zone": "field"}},
        {"zone_free": {"who": "self", "zone": {"param": "a"}}},
    ],
)
def test_parameter_placement_is_local(
    schema: dict[str, JsonValue], value: dict[str, JsonValue]
) -> None:
    _reject(schema, E, value)
    wrapped: JsonValue = {"not": {"cmp": {"left": value, "op": "eq", "right": 0}}}
    _reject(schema, C, wrapped)


@pytest.mark.parametrize("operand", ["left", "right"])
def test_cmp_rejects_direct_parameters(
    schema: dict[str, JsonValue], operand: str
) -> None:
    payload: dict[str, JsonValue] = {"left": 1, "op": "eq", "right": 2}
    payload[operand] = {"param": "n"}
    _reject(schema, "A.cond.cmp", {"cmp": payload})
    _reject(schema, C, {"cmp": payload})


def test_deep_parameters_still_use_their_own_position(
    schema: dict[str, JsonValue],
) -> None:
    value: JsonValue = {
        "not": {
            "cmp": {
                "left": {"times": [2, {"plus": ["X", {"param": "offset"}]}]},
                "op": "le",
                "right": {"minus": ["Y", {"param": "deduction"}]},
            }
        }
    }
    _accept(schema, C, value)


@pytest.mark.parametrize("op", ["eq", "ne", "lt", "le", "gt", "ge"])
def test_numeric_comparison_operators(schema: dict[str, JsonValue], op: str) -> None:
    _accept(schema, C, {"cmp": {"left": -SAFE, "op": op, "right": SAFE}})


@pytest.mark.parametrize("op", ["contains", "=", "EQ", True, {"param": "op"}])
def test_unknown_comparison_operator(
    schema: dict[str, JsonValue], op: JsonValue
) -> None:
    _reject(schema, C, {"cmp": {"left": 0, "op": op, "right": 1}})


@pytest.mark.parametrize(
    ("left", "op", "right"),
    [
        ({"resource": {"who": "self", "kind": "sep"}}, "eq", 0),
        ({"cost_of": "item"}, "eq", {"bound": "lowest"}),
        ({"cost_of": "found"}, "le", 2),
        ({"turn_number": {"who": "self"}}, "lt", 5),
        ({"zone_free": {"who": "self", "zone": "field"}}, "eq", 0),
    ],
    ids=["gap-005", "gap-028", "gap-055", "gap-056", "supplement-016c"],
)
def test_documented_numeric_fragments(
    schema: dict[str, JsonValue], left: JsonValue, op: str, right: JsonValue
) -> None:
    _accept(schema, C, {"cmp": {"left": left, "op": op, "right": right}})


@pytest.mark.parametrize(
    ("name", "value"),
    [
        ("A.expr.min", {"min_": [1]}),
        ("A.expr.max", {"max_": [1]}),
        ("A.cond.all", {"all_": [CMP]}),
        ("A.cond.any", {"any_": [CMP]}),
        ("A.cond.not", {"not_": CMP}),
    ],
)
def test_python_field_names_are_not_wire_aliases(
    schema: dict[str, JsonValue], name: str, value: JsonValue
) -> None:
    _reject(schema, name, value)


@pytest.mark.parametrize(
    "raw",
    [
        b"1.0",
        b'{"plus":[1,2.0]}',
        b'{"per":{"of":5,"unit":1.0}}',
    ],
    ids=["scalar", "nested_operand", "unit"],
)
def test_floats_are_rejected_before_schema_and_by_strict_decoder(raw: bytes) -> None:
    with pytest.raises(ValueError, match="Floating point"):
        parse(raw)
    with pytest.raises(ValidationError):
        NUMERIC[E].validate_json(raw, strict=True)


def test_fragment_library_rejects_standalone_input(
    schema: dict[str, JsonValue],
) -> None:
    assert not Draft202012Validator(schema).is_valid({"plus": [1, 2]})


def test_expression_rejects_mixed_operators(schema: dict[str, JsonValue]) -> None:
    _reject(schema, E, {"plus": [1, 2], "minus": [3, 1]})
