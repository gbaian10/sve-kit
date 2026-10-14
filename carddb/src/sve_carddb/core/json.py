"""Strict JSON boundary and canonical-json-v1 bytes."""

import hashlib
import json
from typing import TYPE_CHECKING, Never, cast

import orjson

if TYPE_CHECKING:
    from collections.abc import Iterable

    from pydantic import JsonValue

SAFE_INTEGER = 9007199254740991
_SHORT_ESCAPES = (
    (b"\\b", b"\\u0008"),
    (b"\\t", b"\\u0009"),
    (b"\\n", b"\\u000a"),
    (b"\\f", b"\\u000c"),
    (b"\\r", b"\\u000d"),
)


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


def _reject_float(_value: str) -> Never:
    raise ValueError("Floating point JSON is forbidden")


def parse(data: bytes) -> JsonValue:
    """Decode UTF-8 JSON without duplicate keys, floats or non-finite values."""
    value = cast(
        "JsonValue",
        json.loads(
            data.decode("utf-8"),
            object_pairs_hook=_pairs,
            parse_float=_reject_float,
            parse_constant=_reject_float,
        ),
    )
    _check(value)
    return value


def _check(value: JsonValue) -> None:
    pending = [value]
    while pending:
        item = pending.pop()
        if item is None or type(item) is bool:
            continue
        if type(item) is str:
            item.encode("utf-8")
        elif type(item) is int:
            integer(item)
        elif type(item) is list:
            pending.extend(item)
        elif type(item) is dict:
            for key in item:
                if type(key) is not str:
                    raise TypeError("Expected string key")
                key.encode("utf-8")
            pending.extend(item.values())
        elif type(item) is float:
            raise ValueError("Floating point JSON is forbidden")
        else:
            raise TypeError("Unsupported JSON type: " + type(item).__name__)


def canonical(value: JsonValue) -> bytes:
    """Encode the contract's exact canonical bytes, including control escapes."""
    _check(value)
    data = orjson.dumps(value, option=orjson.OPT_SORT_KEYS | orjson.OPT_STRICT_INTEGER)
    # JSON cannot contain literal NUL; protect paired backslashes before rewriting escapes.
    data = data.replace(b"\\\\", b"\x00")
    for short, long in _SHORT_ESCAPES:
        data = data.replace(short, long)
    return data.replace(b"\x00", b"\\\\")


def digest(data: bytes) -> str:
    """Return a prefixed SHA-256 digest of exact bytes."""
    return "sha256:" + hashlib.sha256(data).hexdigest()
