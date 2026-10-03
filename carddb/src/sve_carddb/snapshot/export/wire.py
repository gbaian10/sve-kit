"""Encode only the authoritative fixed nested tuple descriptors."""

from pydantic import JsonValue

from sve_carddb.snapshot.contract import (
    columns,
    definition,
    descriptor,
    required_types,
    row_type,
)
from sve_carddb.snapshot.profiles import LEGACY
from sve_carddb.snapshot.values import array, object_value, string


def _value(kind: dict[str, JsonValue], value: JsonValue) -> JsonValue:
    if "nullable" in kind:
        return None if value is None else _value(object_value(kind["nullable"]), value)
    if "array" in kind:
        return [_value(object_value(kind["array"]), item) for item in array(value)]
    if "ref" in kind:
        return encode(string(kind["ref"]), object_value(value))
    return value


def encode(name: str, row: dict[str, JsonValue]) -> list[JsonValue]:
    """Reject missing/extra fields before emitting positional wire values."""
    names = columns(name)
    if set(row) != set(names):
        raise ValueError(f"Public field whitelist mismatch: {name}")
    return [
        _value(object_value(kind), row[field])
        for field, kind in zip(names, array(definition(name)["x-types"]), strict=True)
    ]


def container(
    tables: dict[str, JsonValue], format_version: str = LEGACY
) -> dict[str, JsonValue]:
    """Attach exactly the transitive nested descriptors used by these fragments."""
    used: set[str] = set()
    for table, fragments in tables.items():
        for fragment in array(fragments):
            used |= required_types(
                row_type(
                    table, string(object_value(fragment)["partition"]), format_version
                )
            )
    return {
        "format_version": format_version,
        "types": {name: descriptor(name) for name in sorted(used)},
        "tables": tables,
    }
