"""Load wheel-packaged schemas and decode only fixed format descriptors."""

from functools import cache
from importlib.resources import files

from jsonschema import Draft202012Validator, FormatChecker
from pydantic import JsonValue

from sve_carddb.snapshot.values import array, canonical, object_value, parse, string


@cache
def schema() -> dict[str, JsonValue]:
    """Load the self-contained schema without network resolution."""
    resource = files("sve_carddb.snapshot").joinpath("schema/v1/contract.schema.json")
    return object_value(parse(resource.read_bytes()))


def definition(name: str) -> dict[str, JsonValue]:
    """Get a named format definition."""
    return object_value(object_value(schema()["$defs"])[name])


def validate(name: str, value: JsonValue) -> None:
    """Validate shape and primitive boundaries against a fixed definition."""
    canonical(value)
    selected = schema() | {"$ref": "#/$defs/" + name}
    selected.pop("oneOf")
    Draft202012Validator(selected, format_checker=FormatChecker()).validate(value)


def columns(name: str) -> list[str]:
    """Return the immutable column order for a row or nested tuple."""
    return [string(item) for item in array(definition(name)["x-columns"])]


def tables() -> list[str]:
    """Return the complete public collection whitelist."""
    return [string(item) for item in array(definition("Container")["x-tables"])]


def row_type(table: str, partition: str) -> str:
    """Resolve a fragment's fixed row type."""
    mapping = object_value(definition("Container")["x-fragments"])
    return string(object_value(mapping[table])[partition])


def descriptor(name: str) -> dict[str, JsonValue]:
    """Return the format-authoritative descriptor, never a payload override."""
    return {
        "columns": definition(name)["x-columns"],
        "items": definition(name)["x-types"],
    }


def _decode_type(kind: dict[str, JsonValue], value: JsonValue) -> JsonValue:
    if "nullable" in kind:
        return (
            None
            if value is None
            else _decode_type(object_value(kind["nullable"]), value)
        )
    if "array" in kind:
        return [
            _decode_type(object_value(kind["array"]), item) for item in array(value)
        ]
    if "ref" in kind:
        return decode(string(kind["ref"]), value)
    return value


def decode(name: str, value: JsonValue) -> dict[str, JsonValue]:
    """Decode a validated tuple with a fixed descriptor."""
    kinds = array(definition(name)["x-types"])
    return {
        column: _decode_type(object_value(kind), item)
        for column, kind, item in zip(columns(name), kinds, array(value), strict=True)
    }


def _references(kind: dict[str, JsonValue]) -> set[str]:
    if "ref" in kind:
        name = string(kind["ref"])
        return {name} | required_types(name)
    for key in ("array", "nullable"):
        if key in kind:
            return _references(object_value(kind[key]))
    return set()


def required_types(name: str) -> set[str]:
    """Compute nested descriptor closure from the fixed, acyclic schema."""
    result: set[str] = set()
    for item in array(definition(name)["x-types"]):
        result |= _references(object_value(item))
    return result
