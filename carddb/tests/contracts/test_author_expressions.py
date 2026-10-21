"""Numeric fragment behavior, including placement, recursive shape and wire keys."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import ValidationError

from sve_carddb.core.json import canonical, object_value, parse

from .author_numeric_cases import CONDITIONS, EXPRESSIONS, NUMERIC, C, E

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


@pytest.mark.parametrize("value", [-SAFE, -1, 0, 1, SAFE, "X", "lowest"])
def test_numeric_scalar_values(schema: dict[str, JsonValue], value: JsonValue) -> None:
    _accept(schema, E, value)
    _reject(schema, C, value)


@pytest.mark.parametrize(
    "value",
    [True, False],
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
    "raw",
    [b"1.0", b'{"plus":[1,2.0]}'],
    ids=["scalar", "nested_operand"],
)
def test_floats_are_rejected_before_schema_and_by_strict_decoder(raw: bytes) -> None:
    with pytest.raises(ValueError, match="Floating point"):
        parse(raw)
    with pytest.raises(ValidationError):
        NUMERIC[E].validate_json(raw, strict=True)


def test_expression_rejects_mixed_operators(schema: dict[str, JsonValue]) -> None:
    _reject(schema, E, {"plus": [1, 2], "minus": [3, 1]})
