"""Synthetic author-boundary cases, independent of the prototype effect data."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import object_value
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_inputs import Leaf

SCHEMAS = Path(__file__).resolve().parents[4] / "dsl"


@pytest.fixture(scope="module")
def schemas() -> AuthorSchemas:
    return AuthorSchemas(SCHEMAS)


@pytest.mark.parametrize("name", ["on", "off", "n", "yes", "no", "X", "offset_2"])
def test_yaml_canary_strings(schemas: AuthorSchemas, name: str) -> None:
    assert (
        object_value(schemas.leaf(f"param: {name}\n".encode(), "A.param"))["param"]
        == name
    )
    result = object_value(
        schemas.leaf(f"type: CardName\nvalue: {name}\n".encode(), "ParamValue")
    )
    assert result["value"] == name


@pytest.mark.parametrize(
    ("raw", "entry"),
    [
        (b"{param: true}", "A.param"),
        (b"{true: x}", "A.param"),
        (b"{param: x, param: y}", "A.param"),
        (
            b"{type: Nat, source: {slot: count, component: value}, default: 1}",
            "ParamDecl",
        ),
    ],
    ids=repr,
)
def test_strict_yaml_and_closed_keys(
    schemas: AuthorSchemas, raw: bytes, entry: Leaf
) -> None:
    with pytest.raises(AuthorInputError):
        schemas.leaf(raw, entry)


def _index(*records: dict[str, str], **overrides: object) -> bytes:
    value = {"format": 1, "kind": "effects_index", "records": records, **overrides}
    return json.dumps(value).encode()


def _record(**overrides: str) -> dict[str, str]:
    return {
        "card_id": "c:" + "a" * 32,
        "scope": "shared",
        "file": "rules/author-effects/SYN/001.yaml",
        "kind": "author_effect_set",
        "record_key": "SYN-001aⓈ",
        **overrides,
    }


def test_index_preserves_exact_identifiers(schemas: AuthorSchemas) -> None:
    index = schemas.effects_index(_index(_record(scope="en_override"), _record()))
    assert index.records[0].record_key == "SYN-001aⓈ"
    assert index.records[0].file == "rules/author-effects/SYN/001.yaml"
    assert [record.scope for record in index.records] == ["en_override", "shared"]
    assert schemas.effects_index(_index()).records == ()
    assert (
        schemas.effects_index(_index(_record(kind="effect_set"))).records[0].kind
        == "effect_set"
    )


def test_index_rejects_unsorted_records_with_sort_order(
    schemas: AuthorSchemas,
) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.effects_index(
            _index(
                _record(record_key="SYN-002"),
                _record(record_key="SYN-001", card_id="c:" + "b" * 32),
            )
        )

    assert caught.value.diagnostic.reason == (
        "records_not_sorted_by_record_key_then_scope"
    )
    assert caught.value.diagnostic.expected == "record_key, then scope"


def test_index_rejects_unsorted_scope_tiebreaker(schemas: AuthorSchemas) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.effects_index(_index(_record(), _record(scope="en_override")))

    assert caught.value.diagnostic.reason == (
        "records_not_sorted_by_record_key_then_scope"
    )


@pytest.mark.parametrize(
    "path",
    [
        "/absolute.yaml",
        "../out.yaml",
        "rules/../out.yaml",
        "./a.yaml",
        "a/.",
        "a/..",
        ".",
        "..",
        "a//b",
        "a/",
        "",
        "a\\b",
        "a\nb",
        "a\x00b",
    ],
    ids=repr,
)
def test_index_paths_are_relative_without_normalizing(
    schemas: AuthorSchemas, path: str
) -> None:
    with pytest.raises(AuthorInputError):
        schemas.effects_index(_index(_record(file=path)))


def test_index_duplicate_diagnostic(schemas: AuthorSchemas) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.effects_index(_index(_record(), _record(file="other.yaml")))
    diagnostic = caught.value.diagnostic
    assert diagnostic.reason == "duplicate_card_scope"
    assert diagnostic.path == ("records", 1)
    assert diagnostic.line == 1


def test_duplicate_key_diagnostic_does_not_expose_input(schemas: AuthorSchemas) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.leaf(b"param: x\nparam: private_value", "A.param")
    diagnostic = caught.value.diagnostic
    assert diagnostic.reason == "duplicate_key"
    assert (diagnostic.line, diagnostic.column) == (2, 1)
    assert "private_value" not in str(caught.value)


@pytest.mark.parametrize(
    ("raw", "reason", "path", "line", "column"),
    [
        (b"dict:\n  true: 1\n", "non_string_key", ("dict",), 2, 3),
        (b"dict:\n  null: 1\n", "non_string_key", ("dict",), 2, 3),
        (
            b"dict:\n  type: CardName\n  value: !custom private_value\n",
            "non_json_value",
            ("dict", "value"),
            3,
            18,
        ),
        (
            b"dict: {type: CardName, value: [!custom private_value]}",
            "non_json_value",
            ("dict", "value", 0),
            1,
            40,
        ),
    ],
    ids=repr,
)
def test_json_diagnostic_uses_structural_paths(
    schemas: AuthorSchemas,
    raw: bytes,
    reason: str,
    path: tuple[str | int, ...],
    line: int,
    column: int,
) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.leaf(raw, "ParamValues")
    diagnostic = caught.value.diagnostic
    assert (diagnostic.stage, diagnostic.reason) == ("json", reason)
    assert diagnostic.path == path
    assert (diagnostic.line, diagnostic.column) == (line, column)
    assert "private_value" not in str(caught.value)


@pytest.mark.parametrize(
    ("raw", "reason", "path", "line", "column"),
    [
        (
            b"dict:\n  type: Int\n  value: 1\n  secret_extra: private_value\n",
            "additionalProperties",
            ("dict", "secret_extra"),
            4,
            17,
        ),
        (
            b"bad-name: {type: Nat, value: 1}",
            "pattern",
            ("bad-name",),
            1,
            11,
        ),
    ],
    ids=repr,
)
def test_schema_diagnostic_identifies_bad_key(
    schemas: AuthorSchemas,
    raw: bytes,
    reason: str,
    path: tuple[str | int, ...],
    line: int,
    column: int,
) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.leaf(raw, "ParamValues")
    diagnostic = caught.value.diagnostic
    assert (diagnostic.stage, diagnostic.reason) == ("schema", reason)
    assert diagnostic.path == path
    assert (diagnostic.line, diagnostic.column) == (line, column)
    assert "private_value" not in str(caught.value)


def test_authored_size_bound(schemas: AuthorSchemas) -> None:
    with pytest.raises(AuthorInputError, match="size_limit"):
        schemas.leaf(b"#" * (1024 * 1024 + 1), "A.param")
