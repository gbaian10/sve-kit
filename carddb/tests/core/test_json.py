"""Exercise the strict JSON boundary and identity hashing with synthetic payloads."""

import json

import pytest

from sve_carddb.contracts.four_layer import hash_payload
from sve_carddb.core.json import canonical, parse


@pytest.mark.parametrize(
    ("data", "error"),
    [
        (b'{"a":[1.0]}', ValueError),
        (b'{"a":[-0.0]}', ValueError),
        (b'{"a":[1e400]}', ValueError),
        (b'{"a":[NaN]}', ValueError),
        (b'{"a":[Infinity]}', ValueError),
        (b'{"a":[-Infinity]}', ValueError),
        (b'{"a":[{"k":1,"k":2}]}', ValueError),
        (b'{"a":[9007199254740992]}', ValueError),
        (b'{"a":[-9007199254740992]}', ValueError),
        (b'{"a":["\\ud800"]}', UnicodeEncodeError),
        (b'{"a":[{"\\udfff":null}]}', UnicodeEncodeError),
        (b"{1:2}", json.JSONDecodeError),
        (b'{"a":"\xff"}', UnicodeDecodeError),
    ],
    ids=[
        "float",
        "negative-zero",
        "exponent",
        "nan",
        "infinity",
        "negative-infinity",
        "duplicate-key",
        "unsafe-positive",
        "unsafe-negative",
        "surrogate-value",
        "surrogate-key",
        "nonstring-key",
        "non-utf8",
    ],
)
def test_parse_rejects_invalid_payloads(data: bytes, error: type[Exception]) -> None:
    with pytest.raises(error):
        parse(data)


@pytest.mark.parametrize(
    "data",
    [
        b"null",
        b"true",
        b"-9007199254740991",
        b'{ "z": [9007199254740991, false, null, {"": "\\ud83d\\ude00"}], "a": "\\u0000" }',
    ],
    ids=["null", "boolean", "safe-negative", "nested-unicode"],
)
def test_parse_preserves_json_values_and_canonical_bytes(data: bytes) -> None:
    value = parse(data)
    assert value == json.loads(data)
    assert canonical(parse(canonical(value))) == canonical(value)


@pytest.mark.parametrize(
    ("value", "error"),
    [
        ({"a": [1.0]}, ValueError),
        ({"a": [float("nan")]}, ValueError),
        ({"a": [float("inf")]}, ValueError),
        ({"a": [9007199254740992]}, ValueError),
        ({"a": ["\ud800"]}, UnicodeEncodeError),
        ({"\udfff": None}, UnicodeEncodeError),
        ({1: 2}, AttributeError),
        ((1, 2), TypeError),
    ],
    ids=[
        "float",
        "nan",
        "infinity",
        "unsafe-integer",
        "surrogate-value",
        "surrogate-key",
        "nonstring-key",
        "tuple",
    ],
)
def test_hash_payload_rejects_invalid_values(
    value: object, error: type[Exception]
) -> None:
    with pytest.raises(error):
        hash_payload(value)
