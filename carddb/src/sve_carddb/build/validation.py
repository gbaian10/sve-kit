"""Validate stored values without SQLite affinity coercion at the Python boundary."""

import re
from typing import TYPE_CHECKING

from jsonschema import Draft202012Validator, ValidationError
from pydantic import JsonValue

from sve_carddb.build.model import Column, Json, Kind, Value
from sve_carddb.core.json import SAFE_INTEGER, canonical, parse

if TYPE_CHECKING:
    from collections.abc import Iterator, Mapping

BOUNDS = {
    Kind.INT: (-SAFE_INTEGER, SAFE_INTEGER),
    Kind.UINT: (0, SAFE_INTEGER),
    Kind.UINT32: (0, 4294967295),
}
type SQLValue = str | int | None


def _subschemas(value: dict[str, JsonValue]) -> Iterator[JsonValue]:
    for key in (
        "$defs",
        "definitions",
        "properties",
        "patternProperties",
        "dependentSchemas",
    ):
        children = value.get(key)
        if isinstance(children, dict):
            yield from children.values()
    for key in ("allOf", "anyOf", "oneOf", "prefixItems"):
        children = value.get(key)
        if isinstance(children, list):
            yield from children
    for key in (
        "items",
        "contains",
        "additionalProperties",
        "unevaluatedProperties",
        "propertyNames",
        "not",
        "if",
        "then",
        "else",
        "unevaluatedItems",
        "contentSchema",
    ):
        if key in value:
            yield value[key]


def _local_schema(value: JsonValue) -> None:
    if not isinstance(value, dict):
        return
    if "$id" in value or "format" in value:
        raise ValueError("Use local JSON schemas and explicit constraints")
    for key in ("$ref", "$dynamicRef"):
        target = value.get(key)
        if key in value and (not isinstance(target, str) or not target.startswith("#")):
            raise ValueError("External JSON schema references are forbidden")
    # const/default/examples contain data, not recursively evaluated schemas.
    for child in _subschemas(value):
        _local_schema(child)


class Rules:
    """Pin validators to serialized declarations; never fetch schemas from a URL."""

    def __init__(self, schemas: Mapping[str, str]) -> None:
        self._validators: dict[str, Draft202012Validator] = {}
        for name, data in schemas.items():
            value = parse(data.encode())
            if not isinstance(value, (dict, bool)):
                raise TypeError("JSON schema must be an object or boolean")
            _local_schema(value)
            Draft202012Validator.check_schema(value)
            self._validators[name] = Draft202012Validator(value)

    def validate(self, name: str, value: JsonValue) -> None:
        """Validate a named rule after checking canonical JSON scalar limits."""
        canonical(value)
        self._validators[name].validate(value)

    def sql_json(self, name: object, data: object) -> int:
        """Return a fail-closed SQLite CHECK result for malformed JSON."""
        if not isinstance(name, str) or not isinstance(data, str):
            return 0
        try:
            self.validate(name, parse(data.encode()))
        except ValueError, TypeError, KeyError, ValidationError:
            return 0
        return 1


def fullmatch(pattern: object, value: object) -> int:
    """Use full-string matches, not SQLite LIKE or repair of malformed strings."""
    if not isinstance(pattern, str) or not isinstance(value, str):
        return 0
    return int(re.fullmatch(pattern, value) is not None)


def _text(column: Column, value: Value) -> str:
    if not isinstance(value, str):
        raise TypeError(f"Expected text for {column.name}")
    value.encode("utf-8")
    if column.kind == Kind.ID and not value:
        raise ValueError("Empty ID")
    if column.fixed is not None and value != column.fixed:
        raise ValueError("Fixed column value cannot change")
    if column.choices and value not in column.choices:
        raise ValueError("Unknown enum value")
    if column.pattern is not None and not fullmatch(column.pattern, value):
        raise ValueError("Text does not match column pattern")
    return value


def encode(column: Column, value: Value, rules: Rules) -> SQLValue:
    """Check a Python value before binding it, excluding bool-as-int and coercion."""
    if value is None:
        if not column.nullable:
            raise ValueError(f"Non-nullable column: {column.name}")
        return None
    if column.kind in BOUNDS:
        low, high = BOUNDS[column.kind]
        if type(value) is not int or not low <= value <= high:
            raise ValueError(f"Integer outside domain: {column.name}")
        return value
    if column.kind == Kind.BOOL:
        if not isinstance(value, bool):
            raise TypeError(f"Expected bool: {column.name}")
        return int(value)
    if column.kind == Kind.JSON:
        if column.json_schema is None:
            raise ValueError("Missing JSON schema")
        if not isinstance(value, Json):
            raise TypeError("JSON columns require an explicit Json value")
        rules.validate(column.json_schema, value.value)
        return canonical(value.value).decode()
    return _text(column, value)


def decode(column: Column, value: object, rules: Rules) -> Value:
    """Check raw SQLite storage classes and reconstruct logical Bool/JSON values."""
    if value is None:
        encode(column, None, rules)
        return None
    if column.kind in {*BOUNDS, Kind.BOOL}:
        if type(value) is not int:
            raise TypeError("SQLite returned a non-integer storage class")
        if column.kind == Kind.BOOL:
            if value not in {0, 1}:
                raise ValueError("SQLite returned an invalid boolean")
            return bool(value)
        encode(column, value, rules)
        return value
    if not isinstance(value, str):
        raise TypeError("SQLite returned a non-text storage class")
    if column.kind == Kind.JSON:
        parsed = Json(parse(value.encode()))
        encode(column, parsed, rules)
        return parsed
    return _text(column, value)
