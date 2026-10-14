"""Load wheel-packaged schemas and decode only fixed format descriptors."""

from functools import cache
from importlib.resources import files

import jsonschema_rs
from jsonschema import ValidationError
from pydantic import JsonValue

from sve_carddb.contracts.profiles import MEDIA, profile
from sve_carddb.core.json import array, canonical, object_value, parse, string


@cache
def schema(format_version: str = MEDIA) -> dict[str, JsonValue]:
    """Load the self-contained schema without network resolution."""
    resource = files("sve_carddb.contracts").joinpath(
        "schema/" + profile(format_version).resource + "/contract.schema.json"
    )
    return object_value(parse(resource.read_bytes()))


def definition(name: str, format_version: str = MEDIA) -> dict[str, JsonValue]:
    """Get a named format definition."""
    return object_value(object_value(schema(format_version)["$defs"])[name])


def compile_validator(
    selected: dict[str, JsonValue] | bool,
) -> jsonschema_rs.Draft202012Validator:
    """Compile an offline schema with explicit constraints and the default regex backend."""
    if isinstance(selected, dict):
        validator = jsonschema_rs.Draft202012Validator(
            selected, validate_formats=False, offline=True
        )
    else:
        validator = jsonschema_rs.Draft202012Validator(
            selected, validate_formats=False, offline=True
        )
    return validator


def validate_schema(
    validator: jsonschema_rs.Draft202012Validator, value: JsonValue
) -> None:
    """Preserve the existing exception contract without revalidating failures."""
    try:
        validator.validate(value)
    except jsonschema_rs.ValidationError as error:
        raise ValidationError(
            error.message,
            path=error.instance_path,
            schema_path=error.schema_path,
            validator=error.kind.name,
            instance=error.instance,
        ) from error


@cache
def _validator(name: str, format_version: str) -> jsonschema_rs.Draft202012Validator:
    selected = schema(format_version) | {"$ref": "#/$defs/" + name}
    selected.pop("oneOf")
    return compile_validator(selected)


def validate(name: str, value: JsonValue, format_version: str = MEDIA) -> None:
    """Validate shape and primitive boundaries against a fixed definition."""
    canonical(value)
    validate_schema(_validator(name, format_version), value)


def columns(name: str, format_version: str = MEDIA) -> list[str]:
    """Return the immutable column order for a row or nested tuple."""
    return [
        string(item) for item in array(definition(name, format_version)["x-columns"])
    ]


def tables() -> list[str]:
    """Return the complete public collection whitelist."""
    return [string(item) for item in array(definition("Container")["x-tables"])]


def row_type(table: str, partition: str, format_version: str = MEDIA) -> str:
    """Resolve a fragment's fixed row type."""
    mapping = object_value(definition("Container", format_version)["x-fragments"])
    return string(object_value(mapping[table])[partition])


def descriptor(name: str, format_version: str = MEDIA) -> dict[str, JsonValue]:
    """Return the format-authoritative descriptor, never a payload override."""
    return {
        "columns": definition(name, format_version)["x-columns"],
        "items": definition(name, format_version)["x-types"],
    }


def _decode_type(
    kind: dict[str, JsonValue], value: JsonValue, format_version: str = MEDIA
) -> JsonValue:
    if "nullable" in kind:
        return (
            None
            if value is None
            else _decode_type(object_value(kind["nullable"]), value, format_version)
        )
    if "array" in kind:
        return [
            _decode_type(object_value(kind["array"]), item, format_version)
            for item in array(value)
        ]
    if "ref" in kind:
        return decode(string(kind["ref"]), value, format_version)
    return value


def decode(
    name: str, value: JsonValue, format_version: str = MEDIA
) -> dict[str, JsonValue]:
    """Decode a validated tuple with a fixed descriptor."""
    kinds = array(definition(name, format_version)["x-types"])
    return {
        column: _decode_type(object_value(kind), item, format_version)
        for column, kind, item in zip(
            columns(name, format_version), kinds, array(value), strict=True
        )
    }


def _references(kind: dict[str, JsonValue], format_version: str = MEDIA) -> set[str]:
    if "ref" in kind:
        name = string(kind["ref"])
        return {name} | required_types(name, format_version)
    for key in ("array", "nullable"):
        if key in kind:
            return _references(object_value(kind[key]), format_version)
    return set()


def required_types(name: str, format_version: str = MEDIA) -> set[str]:
    """Compute nested descriptor closure from the fixed, acyclic schema."""
    result: set[str] = set()
    for item in array(definition(name, format_version)["x-types"]):
        result |= _references(object_value(item), format_version)
    return result
