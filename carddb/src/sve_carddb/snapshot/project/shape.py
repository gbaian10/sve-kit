"""Validate logical records against the same fixed descriptors as the wire contract."""

from pydantic import JsonValue

from sve_carddb.snapshot.contract import columns, definition, validate
from sve_carddb.snapshot.values import array, object_value, string


def _value(kind: dict[str, JsonValue], value: JsonValue) -> JsonValue:
    if "nullable" in kind:
        return None if value is None else _value(object_value(kind["nullable"]), value)
    if "array" in kind:
        return [_value(object_value(kind["array"]), item) for item in array(value)]
    if "ref" in kind:
        return tuple_value(string(kind["ref"]), object_value(value))
    return value


def tuple_value(name: str, record: dict[str, JsonValue]) -> list[JsonValue]:
    """Use tuples only as an internal shape check, not a transport exporter."""
    names = columns(name)
    if set(record) != set(names):
        raise ValueError(f"Public field whitelist mismatch: {name}")
    result = [
        _value(object_value(kind), record[field])
        for field, kind in zip(names, array(definition(name)["x-types"]), strict=True)
    ]
    validate(name, result)
    return result
