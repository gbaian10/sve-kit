"""Check logical fields and constraints not covered by wire row validation."""

from pydantic import JsonValue

from sve_carddb.contracts.snapshot import columns, definition, validate
from sve_carddb.core.json import array, object_value, string


def _value(kind: dict[str, JsonValue], value: JsonValue) -> JsonValue:
    if "nullable" in kind:
        return None if value is None else _value(object_value(kind["nullable"]), value)
    if "array" in kind:
        return [_value(object_value(kind["array"]), item) for item in array(value)]
    if "ref" in kind:
        return tuple_value(string(kind["ref"]), object_value(value))
    return value


def tuple_value(name: str, record: dict[str, JsonValue]) -> list[JsonValue]:
    """Check field whitelists and logical constraints before wire partitioning."""
    names = columns(name)
    if set(record) != set(names):
        raise ValueError(f"Public field whitelist mismatch: {name}")
    result = [
        _value(object_value(kind), record[field])
        for field, kind in zip(names, array(definition(name)["x-types"]), strict=True)
    ]
    # Split rows and logical-only allOf rules are not covered by Container validation.
    if name in {"printing", "face_revision", "annotation_concept", "field_annotation"}:
        validate(name, result)
    return result
