"""Synthetic author-boundary cases, independent of the prototype effect data."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.yaml import MAX_BYTES
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_inputs import Leaf

SCHEMAS = Path(__file__).resolve().parents[4] / "dsl"


@pytest.fixture(scope="module")
def schemas() -> AuthorSchemas:
    return AuthorSchemas(SCHEMAS)


@pytest.mark.parametrize(
    ("kind", "value"),
    [
        ("Int", -9007199254740991),
        ("Int", -2),
        ("Int", 9007199254740991),
        ("Nat", 0),
        ("Nat", 9007199254740991),
        ("CardName", "合成試験名"),
        ("Concept", "ward"),
        ("Concept", "合成種族"),
        ("Concept", "試験職業"),
        ("CardKind", "follower"),
        ("Player", "self"),
        ("Player", "opp"),
        ("Zone", "field"),
        ("Zone", "hand"),
        ("Zone", "deck"),
        ("Zone", "evolve_deck"),
        ("Zone", "cemetery"),
        ("Zone", "ex"),
        ("Phase", "start"),
        ("Phase", "main"),
        ("Phase", "end"),
    ],
    ids=repr,
)
def test_typed_values(schemas: AuthorSchemas, kind: str, value: str | int) -> None:
    result = schemas.parameter_value(
        json.dumps({"type": kind, "value": value}).encode()
    )
    assert result.type == kind
    assert result.value == value
    assert type(result.value) is type(value)


@pytest.mark.parametrize(
    "raw",
    [
        b"{type: Int, value: true}",
        b"{type: Int, value: '2'}",
        b"{type: Int, value: 2.0}",
        b"{type: Int, value: -9007199254740992}",
        b"{type: Nat, value: 9007199254740992}",
        b"{type: Nat, value: -1}",
        b"{type: Nat, value: false}",
        b"{type: CardName, value: ''}",
        b"{type: Concept, value: ''}",
        b"{type: CardKind, value: []}",
        b"{type: Player, value: opponents}",
        b"{type: Zone, value: banish}",
        b"{type: Phase, value: combat}",
        b"{type: Expr, value: {minus: [1, 2]}}",
        b"{type: Int, value: {minus: [1, 2]}}",
        b"{type: Int, value: 1, note: ignored}",
        b"{type: Int}",
    ],
    ids=repr,
)
def test_reject_value_shapes(schemas: AuthorSchemas, raw: bytes) -> None:
    with pytest.raises(AuthorInputError):
        schemas.parameter_value(raw)


@pytest.mark.parametrize("name", ["on", "off", "n", "yes", "no", "X", "offset_2"])
def test_yaml_canary_strings(schemas: AuthorSchemas, name: str) -> None:
    assert schemas.parameter_reference(f"param: {name}\n".encode()).param == name
    result = schemas.parameter_value(f"type: CardName\nvalue: {name}\n".encode())
    assert result.value == name


def test_date_has_no_implicit_timestamp(schemas: AuthorSchemas) -> None:
    assert (
        schemas.parameter_value(b"type: CardName\nvalue: 2026-01-01").value
        == "2026-01-01"
    )


@pytest.mark.parametrize(
    ("raw", "entry"),
    [
        (b"{param: true}", "A.param"),
        (b"{true: x}", "A.param"),
        (b"{'true': x}", "A.param"),
        (b"{false: x}", "A.param"),
        (b"{1: x}", "A.param"),
        (b"{null: x}", "A.param"),
        (b"{[a,b]: x}", "A.param"),
        (b"{param: x, param: y}", "A.param"),
        (b"{param: !unknown x}", "A.param"),
        (b"%YAML 1.1\n---\nparam: x", "A.param"),
        (b"%YAML 1.3\n---\nparam: x", "A.param"),
        (b"---\nparam: x\n---\nparam: y", "A.param"),
        (b"\xff", "A.param"),
        (b"", "A.param"),
        (b"{param: &cycle [*cycle]}", "A.param"),
        (b"{type: Int, value: .nan}", "ParamValue"),
        (b"{type: Int, value: .inf}", "ParamValue"),
        (b'{type: CardName, value: "\\uD800"}', "ParamValue"),
        (
            b"{type: Nat, source: {slot: count, component: value}, default: 1}",
            "ParamDecl",
        ),
        (b"{type: Nat, source: {slot: count, component: expression}}", "ParamDecl"),
        (b"{type: ObjectRef, source: {slot: count, component: value}}", "ParamDecl"),
        (b"{param: 'x\ny'}", "A.param"),
        (b"{param: 9}", "A.param"),
        (b"{param: x, do: []}", "A.param"),
        (b"{bad-name: {type: Nat, value: 1}}", "ParamValues"),
        (b"{1: {type: Nat, value: 1}}", "ParamValues"),
        (
            b"{offset: {type: Int, source: {slot: X, component: value}}}",
            "ParamDeclarations",
        ),
    ],
    ids=repr,
)
def test_strict_yaml_and_closed_keys(
    schemas: AuthorSchemas, raw: bytes, entry: Leaf
) -> None:
    with pytest.raises(AuthorInputError):
        schemas.leaf(raw, entry)


def test_yaml_directives_tags_aliases_and_merges(schemas: AuthorSchemas) -> None:
    assert schemas.parameter_reference(b"%YAML 1.2\n---\nparam: !!str n").param == "n"
    result = schemas.parameter_values(
        b"first: &first {type: Int, value: 1}\nsecond: {<<: *first, value: 2}\nthird: *first"
    )
    assert [value.value for value in result.values()] == [1, 2, 1]


def test_declarations_and_empty_maps(schemas: AuthorSchemas) -> None:
    raw = b"offset: {type: Int, source: {slot: offset, component: value}}"
    declarations = schemas.parameter_declarations(raw)
    assert declarations["offset"].source.slot == "offset"
    assert (
        schemas.parameter_declaration(
            b"{type: Int, source: {slot: cost, component: negated_magnitude}}"
        ).source.component
        == "negated_magnitude"
    )
    assert schemas.parameter_declarations(b"{}") == {}
    assert schemas.parameter_values(b"{}") == {}


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
    index = schemas.effects_index(_index(_record(), _record(scope="en_override")))
    assert index.records[0].record_key == "SYN-001aⓈ"
    assert index.records[0].file == "rules/author-effects/SYN/001.yaml"
    assert [record.scope for record in index.records] == ["shared", "en_override"]
    assert schemas.effects_index(_index()).records == ()
    assert (
        schemas.effects_index(_index(_record(kind="effect_set"))).records[0].kind
        == "effect_set"
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


@pytest.mark.parametrize(
    "raw",
    [
        _index(_record(scope="en_override", kind="effect_set")),
        _index(_record(scope="jp")),
        _index(_record(kind="effect_body_shard")),
        _index(_record(card_id="c:synthetic")),
        _index(_record(card_id="c:" + "A" * 32)),
        _index(_record(record_key="")),
        _index(_record(), format=True),
        _index(_record(), format="1"),
        _index(_record(), format=2),
        _index(_record(), kind="author_effect_set"),
        _index(_record(), schema="sve-author/1.0"),
        _index(_record(unknown="value")),
    ],
    ids=repr,
)
def test_index_rejects_wrong_envelopes(schemas: AuthorSchemas, raw: bytes) -> None:
    with pytest.raises(AuthorInputError):
        schemas.effects_index(raw)


def test_index_duplicate_diagnostic(schemas: AuthorSchemas) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.effects_index(_index(_record(), _record(file="other.yaml")))
    diagnostic = caught.value.diagnostic
    assert diagnostic.reason == "duplicate_card_scope"
    assert diagnostic.path == ("records", 1)
    assert diagnostic.line == 1


def test_schema_location_and_type_diagnostics(schemas: AuthorSchemas) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.parameter_value(b"type: Int\nvalue: true\n")
    diagnostic = caught.value.diagnostic
    assert diagnostic.stage == "schema"
    assert diagnostic.path == ("value",)
    assert (diagnostic.line, diagnostic.column) == (2, 8)
    assert diagnostic.expected == "integer"
    assert diagnostic.actual == "bool"


def test_duplicate_key_diagnostic_does_not_expose_input(schemas: AuthorSchemas) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.parameter_reference(b"param: x\nparam: private_value")
    diagnostic = caught.value.diagnostic
    assert diagnostic.reason == "duplicate_key"
    assert (diagnostic.line, diagnostic.column) == (2, 1)
    assert "private_value" not in str(caught.value)


def test_noncanonical_location(schemas: AuthorSchemas) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.parameter_value(b"type: Int\nvalue: 2.5")
    assert caught.value.diagnostic.path == ("value",)
    assert caught.value.diagnostic.line == 2


@pytest.mark.parametrize("key", ["true", "false", "null", "True"])
def test_nonstring_parameter_keys_cannot_be_stringified(
    schemas: AuthorSchemas, key: str
) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.parameter_values(f"{key}: {{type: Nat, value: 1}}".encode())
    diagnostic = caught.value.diagnostic
    assert (diagnostic.stage, diagnostic.reason) == ("json", "non_string_key")
    assert diagnostic.path == ()
    assert (diagnostic.line, diagnostic.column) == (1, 1)
    assert (
        schemas.parameter_values(f"'{key}': {{type: Nat, value: 1}}".encode())[
            key
        ].value
        == 1
    )


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
        schemas.parameter_values(raw)
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
        schemas.parameter_values(raw)
    diagnostic = caught.value.diagnostic
    assert (diagnostic.stage, diagnostic.reason) == ("schema", reason)
    assert diagnostic.path == path
    assert (diagnostic.line, diagnostic.column) == (line, column)
    assert "private_value" not in str(caught.value)


@pytest.mark.parametrize(
    ("raw", "reason"),
    [
        (b"param: x\nparam: private_value", "duplicate_key"),
        (b"{[a,b]: x}", "complex_key"),
        (b"%YAML 1.1\n---\nparam: x", "invalid_document"),
        (b"%YAML 1.3\n---\nparam: x", "invalid_document"),
        (b"---\nparam: x\n---\nparam: y", "invalid_document"),
    ],
    ids=repr,
)
def test_yaml_diagnostic_categories(
    schemas: AuthorSchemas, raw: bytes, reason: str
) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.parameter_reference(raw)
    assert (caught.value.diagnostic.stage, caught.value.diagnostic.reason) == (
        "yaml",
        reason,
    )


def test_decoder_drift_does_not_expose_input(tmp_path: Path) -> None:
    for source in SCHEMAS.glob("*.schema.json"):
        (tmp_path / source.name).write_bytes(source.read_bytes())
    path = tmp_path / "author-nodes.schema.json"
    schema = json.loads(path.read_bytes())
    schema["$defs"]["A.param"]["additionalProperties"] = True
    path.write_text(json.dumps(schema))
    with pytest.raises(AuthorInputError) as caught:
        AuthorSchemas(tmp_path).parameter_reference(b"param: x\nextra: private_value")
    assert (caught.value.diagnostic.stage, caught.value.diagnostic.reason) == (
        "schema",
        "decoder_mismatch",
    )
    assert "private_value" not in str(caught.value)


def test_authored_size_bound(schemas: AuthorSchemas) -> None:
    with pytest.raises(AuthorInputError, match="size_limit"):
        schemas.parameter_reference(b"#" * MAX_BYTES)


def test_resource_identity_is_pinned(tmp_path: Path) -> None:
    (tmp_path / "author-nodes.schema.json").write_text('{"$id":"urn:other"}')
    with pytest.raises(ValueError, match="schema identity"):
        AuthorSchemas(tmp_path)
