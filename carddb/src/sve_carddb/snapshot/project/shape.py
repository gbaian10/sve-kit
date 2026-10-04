"""Validate logical records against the same fixed descriptors as the wire contract."""

import re

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


def source_tuple(name: str, record: dict[str, JsonValue]) -> None:
    """Validate current build inputs before media versions are reserved."""
    if name == "printing_image":
        if set(record) != {"printing_id", "face_id", "image_id"}:
            raise ValueError("Source printing image whitelist mismatch")
        for value in record.values():
            validate("ID", value)
    elif name == "image_variant":
        if set(record) != {*columns(name), "path"}:
            raise ValueError("Source image variant whitelist mismatch")
        validate("Path", record["path"])
        if (
            re.fullmatch(
                r"images/sha256/([0-9a-f]{2})/\1[0-9a-f]{62}\.webp",
                string(record["path"]),
            )
            is None
        ):
            raise ValueError(
                "Source image variant requires a content-addressed WebP path"
            )
        tuple_value(
            name, {key: value for key, value in record.items() if key != "path"}
        )
    else:
        tuple_value(name, record)
