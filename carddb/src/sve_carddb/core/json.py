"""Strict JSON boundary and canonical-json-v1 bytes."""

import hashlib
import json
from typing import TYPE_CHECKING

from pydantic import JsonValue, TypeAdapter

if TYPE_CHECKING:
    from collections.abc import Iterable

SAFE_INTEGER = 9007199254740991
_ADAPTER: TypeAdapter[JsonValue] = TypeAdapter(JsonValue)
_ESCAPES = {i: f"\\u{i:04x}" for i in range(32)} | {34: '\\"', 92: "\\\\"}


def object_value(value: JsonValue) -> dict[str, JsonValue]:
    """Require an object at an untrusted boundary."""
    if not isinstance(value, dict):
        raise TypeError("Expected object")
    return value


def array(value: JsonValue) -> list[JsonValue]:
    """Require an array at an untrusted boundary."""
    if not isinstance(value, list):
        raise TypeError("Expected array")
    return value


def string(value: JsonValue) -> str:
    """Require a string at an untrusted boundary."""
    if not isinstance(value, str):
        raise TypeError("Expected string")
    return value


def integer(value: JsonValue) -> int:
    """Exclude booleans and unsafe integers."""
    if type(value) is not int or abs(value) > SAFE_INTEGER:
        raise ValueError("Expected safe integer")
    return value


def _pairs(pairs: Iterable[tuple[str, JsonValue]]) -> dict[str, JsonValue]:
    result: dict[str, JsonValue] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("Duplicate JSON key")
        result[key] = value
    return result


def parse(data: bytes) -> JsonValue:
    """Decode UTF-8 JSON without duplicate keys, floats or non-finite values."""
    raw: object = json.loads(data.decode("utf-8"), object_pairs_hook=_pairs)
    value = _ADAPTER.validate_python(raw)
    canonical(value)
    return value


def _quoted(value: str) -> str:
    value.encode("utf-8")
    return '"' + value.translate(_ESCAPES) + '"'


def _encode(value: JsonValue) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str):
        return _quoted(value)
    if isinstance(value, int):
        return str(integer(value))
    if isinstance(value, list):
        return "[" + ",".join(_encode(item) for item in value) + "]"
    if isinstance(value, dict):
        return (
            "{"
            + ",".join(
                _quoted(key) + ":" + _encode(value[key]) for key in sorted(value)
            )
            + "}"
        )
    if isinstance(value, float):
        raise ValueError("Floating point JSON is forbidden")  # ruff: ignore[type-check-without-type-error] -- JSON numbers are rejected by value under this recipe
    raise TypeError("Unsupported JSON type: " + type(value).__name__)


def canonical(value: JsonValue) -> bytes:
    """Encode the contract's exact canonical bytes, including control escapes."""
    return _encode(value).encode("utf-8")


def digest(data: bytes) -> str:
    """Return a prefixed SHA-256 digest of exact bytes."""
    return "sha256:" + hashlib.sha256(data).hexdigest()
