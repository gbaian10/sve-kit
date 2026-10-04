"""Encode only the authoritative fixed nested tuple descriptors."""

from pydantic import JsonValue

from sve_carddb.snapshot.contract import (
    columns,
    definition,
    descriptor,
    required_types,
    row_type,
)
from sve_carddb.snapshot.profiles import MEDIA
from sve_carddb.snapshot.values import array, object_value, string


def _value(
    kind: dict[str, JsonValue], value: JsonValue, format_version: str
) -> JsonValue:
    if "nullable" in kind:
        return (
            None
            if value is None
            else _value(object_value(kind["nullable"]), value, format_version)
        )
    if "array" in kind:
        return [
            _value(object_value(kind["array"]), item, format_version)
            for item in array(value)
        ]
    if "ref" in kind:
        return encode(string(kind["ref"]), object_value(value), format_version)
    return value


def encode(
    name: str, row: dict[str, JsonValue], format_version: str = MEDIA
) -> list[JsonValue]:
    """Reject missing/extra fields before emitting positional wire values."""
    names = columns(name, format_version)
    if set(row) != set(names):
        raise ValueError(f"Public field whitelist mismatch: {name}")
    return [
        _value(object_value(kind), row[field], format_version)
        for field, kind in zip(
            names, array(definition(name, format_version)["x-types"]), strict=True
        )
    ]


def container(
    tables: dict[str, JsonValue], format_version: str = MEDIA
) -> dict[str, JsonValue]:
    """Attach exactly the transitive nested descriptors used by these fragments."""
    used: set[str] = set()
    for table, fragments in tables.items():
        for fragment in array(fragments):
            used |= required_types(
                row_type(
                    table, string(object_value(fragment)["partition"]), format_version
                ),
                format_version,
            )
    return {
        "format_version": format_version,
        "types": {name: descriptor(name, format_version) for name in sorted(used)},
        "tables": tables,
    }
