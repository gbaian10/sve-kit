# ruff: file-ignore[subclass-builtin] -- Built-in subclasses are deliberate canonical boundary inputs.
"""Consume shared, handwritten contract fixtures without a producer or database."""

from pathlib import Path
from traceback import format_exception
from typing import cast

import pytest
from jsonschema import Draft202012Validator, ValidationError
from pydantic import JsonValue

from sve_carddb.contracts.generate_schema import generate
from sve_carddb.contracts.snapshot import definition, schema, tables, validate
from sve_carddb.core.json import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)
from sve_carddb.export.buckets import bucket
from sve_carddb.export.project import Projection
from sve_carddb.export.reader import read_snapshot
from sve_carddb.export.transport import Snapshot
from sve_carddb.export.transport.compression import compress

from ..support.snapshot_contract_fixtures import _replace, _reseal, fixture, payloads


class _StringSubclass(str):
    __slots__ = ()


class _IntegerSubclass(int):
    pass


class _FloatSubclass(float):
    pass


class _ListSubclass(list[JsonValue]):
    pass


class _DictSubclass(dict[str, JsonValue]):
    pass


def test_schema_is_valid_and_covers_every_collection() -> None:
    Draft202012Validator.check_schema(schema())
    valid = [object_value(case) for case in array(fixture("schema-valid.json"))]
    assert len(tables()) == 46
    assert set(tables()) <= {string(case["schema"]) for case in valid}
    for case in valid:
        validate(string(case["schema"]), case["value"])


def test_schema_regeneration_matches_committed_bytes() -> None:
    resource = (
        Path(__file__).resolve().parents[2]
        / "src/sve_carddb/contracts/schema/v3/contract.schema.json"
    )
    assert generate() == resource.read_bytes()


def test_schema_requires_no_optional_format_checker() -> None:
    def check(value: JsonValue) -> None:
        if isinstance(value, dict):
            assert not isinstance(value.get("format"), str)
            assert r"\d" not in string(value.get("pattern", ""))
            for item in value.values():
                check(item)
        elif isinstance(value, list):
            for item in value:
                check(item)

    check(schema())
    for raw in array(fixture("schema-invalid-core.json")):
        case = object_value(raw)
        if "raw_json" in case:
            continue
        selected = schema() | {"$ref": "#/$defs/" + string(case["schema"])}
        selected.pop("oneOf")
        with pytest.raises(ValidationError):
            Draft202012Validator(selected).validate(case["value"])


@pytest.mark.parametrize(
    "case",
    array(fixture("schema-invalid-core.json")),
    ids=lambda case: object_value(case)["name"],
)
def test_shared_schema_counterexamples(case: JsonValue) -> None:
    item = object_value(case)
    with pytest.raises((ValueError, ValidationError)):
        validate(
            string(item["schema"]),
            parse(string(item["raw_json"]).encode())
            if "raw_json" in item
            else item["value"],
        )


def test_tuple_length_nullable_and_object_required_fields() -> None:
    for raw in array(fixture("schema-valid.json")):
        case = object_value(raw)
        name = string(case["schema"])
        value = case["value"]
        if isinstance(value, list):
            for changed in (value[:-1], [*value, None]):
                with pytest.raises(ValidationError):
                    validate(name, changed)
            for index, item in enumerate(value):
                if item is None:
                    changed = value.copy()
                    del changed[index]
                    with pytest.raises(ValidationError):
                        validate(name, changed)
        elif isinstance(value, dict):
            for key in value:
                changed_object = value.copy()
                del changed_object[key]
                with pytest.raises(ValidationError):
                    validate(name, changed_object)
            with pytest.raises(ValidationError):
                validate(name, value | {"extra": None})


def test_all_fixed_enums_reject_unknown_values() -> None:
    for raw in array(fixture("schema-valid.json")):
        case = object_value(raw)
        name = string(case["schema"])
        if not isinstance(case["value"], list):
            continue
        for index, raw_type in enumerate(array(definition(name)["x-types"])):
            kind = object_value(raw_type)
            if "nullable" in kind:
                kind = object_value(kind["nullable"])
            if "enum" in kind:
                changed = array(case["value"]).copy()
                changed[index] = "unknown-contract-value"
                with pytest.raises(ValidationError):
                    validate(name, changed)


def test_golden_join_matches_independent_logical_view() -> None:
    assert read_snapshot(fixture("manifest.json"), payloads()) == fixture(
        "expected-logical.json"
    )


@pytest.mark.parametrize("failure", ["row", "fragment"])
def test_reader_container_errors_hide_fragment_text(failure: str) -> None:
    manifest = object_value(fixture("manifest.json"))
    values = {key: parse(raw) for key, raw in payloads().items()}
    fragment = next(
        object_value(entry)
        for value in values.values()
        for entry in array(
            object_value(object_value(value).get("tables", {})).get("text_unit", [])
        )
        if object_value(entry)["partition"] == "detail"
    )
    original = array(array(fragment["rows"])[0])
    texts = [f"Synthetic private wording {index}" for index in range(51)]
    rows: list[JsonValue] = [[*original[:2], text] for text in texts]
    fragment["rows"] = rows
    if failure == "row":
        array(rows[25])[2] = None
    else:
        fragment["extra"] = "Synthetic unexpected value"
    blobs = _reseal(manifest, values)

    with pytest.raises(ValidationError) as caught:
        read_snapshot(manifest, blobs)

    error = caught.value
    rendered = "".join(format_exception(error))
    assert all(text not in rendered for text in texts)
    assert "Synthetic unexpected value" not in rendered
    assert error.instance is None
    assert error.__cause__ is None
    assert error.__context__ is None
    assert list(error.absolute_path)[:2] == ["tables", "text_unit"]
    assert "schema_path=" in error.message
    if failure == "row":
        assert "'rows', 25, 2]" in error.message
        assert "type:" in error.message
    else:
        assert "additionalProperties:" in error.message


@pytest.mark.parametrize(
    "case",
    array(fixture("reader-invalid-core.json")),
    ids=lambda case: object_value(case)["name"],
)
def test_shared_reader_counterexamples(case: JsonValue) -> None:
    item = object_value(case)

    manifest = object_value(fixture("manifest.json"))
    values = {key: parse(raw) for key, raw in payloads().items()}
    for raw in [*array(item.get("setup", [])), item]:
        mutation = object_value(raw)
        target = string(mutation["target"])
        _replace(
            manifest if target == "manifest" else values[target],
            array(mutation["path"]),
            mutation["value"],
        )
    blobs = _reseal(manifest, values)
    if item["target"] == "manifest":
        _replace(manifest, array(item["path"]), item["value"])
    if not item.get("rehash", True) and item["target"] != "manifest":
        target = string(item["target"])
        original_file = next(
            object_value(f)
            for f in array(object_value(fixture("manifest.json"))["files"])
            if object_value(f)["key"] == target
        )
        current_file = next(
            object_value(f)
            for f in array(manifest["files"])
            if object_value(f)["key"] == target
        )
        current_file.update(
            {key: original_file[key] for key in ("sha256", "bytes", "path")}
        )
    with pytest.raises(
        (ValueError, ValidationError, KeyError, TypeError),
        match=string(item["error"]) if "error" in item else None,
    ):
        read_snapshot(manifest, blobs)


def test_missing_programs_rejected() -> None:
    manifest = object_value(fixture("manifest.json"))
    manifest["files"] = [
        item
        for item in array(manifest["files"])
        if object_value(item)["role"] != "programs"
    ]
    blobs = payloads()
    del blobs["programs"]
    with pytest.raises(ValueError, match="programs"):
        read_snapshot(manifest, blobs)


def test_dependency_cycle_rejected() -> None:
    manifest = object_value(fixture("manifest.json"))
    files = {
        string(object_value(item)["key"]): object_value(item)
        for item in array(manifest["files"])
    }
    for key, target in [("config", "programs"), ("programs", "config")]:
        files[key]["dependencies"] = [
            {"key": target, "sha256": files[target]["sha256"]}
        ]
    with pytest.raises(ValueError, match="Cyclic"):
        read_snapshot(manifest, payloads())


def test_payload_bytes_cannot_be_reformatted_or_changed() -> None:
    for data in (b"{}", canonical(fixture("payloads/programs.json")) + b"\n"):
        blobs = payloads()
        blobs["programs"] = data
        with pytest.raises(ValueError, match="Blob"):
            read_snapshot(fixture("manifest.json"), blobs)


def test_fixed_canonical_and_bucket_vectors() -> None:
    vectors = object_value(fixture("vectors.json"))
    vector = object_value(vectors["bucket"])
    data = canonical(vector["key"])
    assert data.hex() == vector["bytes_hex"]
    assert digest(data) == vector["sha256"]
    assert bucket(array(vector["key"]), 4) == vector["expected"]
    for raw in array(vectors["canonical"]):
        case = object_value(raw)
        assert canonical(case["value"]) == string(case["text"]).encode()


@pytest.mark.parametrize(
    "data",
    [
        b'{"a":1,"a":2}',
        b"1.0",
        b"NaN",
        b"9007199254740992",
        b'"\\ud800"',
        b"\xef\xbb\xbf{}",
        b'{"a":[9007199254740992]}',
        b'{"a":["\\ud800"]}',
        b'{"a":[{"\\udfff":null}]}',
    ],
    ids=[
        "duplicate",
        "float",
        "nan",
        "unsafe",
        "surrogate",
        "bom",
        "nested-unsafe",
        "nested-surrogate",
        "surrogate-key",
    ],
)
def test_invalid_json_boundary(data: bytes) -> None:
    with pytest.raises((ValueError, UnicodeError)):
        parse(data)


@pytest.mark.parametrize("value", [1.0, float("nan"), float("inf")])
def test_canonical_rejects_floating_point_numbers(value: float) -> None:
    with pytest.raises(ValueError, match=r"^Floating point JSON is forbidden$"):
        canonical(value)


@pytest.mark.parametrize(
    "value",
    [
        (1, 2),
        {1, 2},
        Path("synthetic"),
        _StringSubclass("synthetic"),
        _IntegerSubclass(1),
        _FloatSubclass(1.0),
        _ListSubclass([1]),
        _DictSubclass(key=1),
    ],
)
def test_canonical_identifies_unsupported_types(value: object) -> None:
    with pytest.raises(
        TypeError, match=r"^Unsupported JSON type: " + type(value).__name__
    ):
        canonical(cast("JsonValue", value))


@pytest.mark.parametrize(
    "value",
    [
        {1: 2},
        {_StringSubclass("key"): 1},
        {"nested": [(1, 2)]},
        {"nested": [9007199254740992]},
        {"nested": ["\ud800"]},
        {"\udfff": None},
    ],
    ids=[
        "integer-key",
        "subclass-key",
        "tuple",
        "unsafe",
        "surrogate",
        "surrogate-key",
    ],
)
def test_canonical_rejects_invalid_nested_values(value: object) -> None:
    with pytest.raises((TypeError, ValueError)):
        canonical(cast("JsonValue", value))


def test_resources_available_from_package() -> None:
    assert schema()["$id"] == "urn:sve-kit:snapshot:4.0.0"
    assert len(canonical(schema())) < 1024 * 1024


def test_dangling_vocabulary_is_not_repaired() -> None:

    manifest = object_value(fixture("manifest.json"))
    values = {key: parse(raw) for key, raw in payloads().items()}
    bootstrap = next(
        object_value(v)
        for key, v in values.items()
        if key.startswith("bootstrap/")
        and "face_revision" in object_value(object_value(v)["tables"])
    )
    fragment = object_value(
        array(object_value(bootstrap["tables"])["face_revision"])[0]
    )
    array(array(fragment["rows"])[0])[5] = "missing_type"
    with pytest.raises(ValueError, match="Vocabulary reference missing"):
        read_snapshot(manifest, _reseal(manifest, values))


@pytest.mark.parametrize("member", ["config", "container"])
def test_matching_member_hash_cannot_bypass_payload_schema(member: str) -> None:
    manifest = object_value(fixture("manifest.json"))
    values = {key: parse(raw) for key, raw in payloads().items()}
    key = (
        "config"
        if member == "config"
        else next(k for k in values if k.startswith("bootstrap/"))
    )
    object_value(values[key])["unexpected"] = True
    blobs = _reseal(manifest, values)
    snapshot = Snapshot(
        manifest,
        {key: compress(value, None) for key, value in blobs.items()},
        {},
        compress(canonical(manifest), None),
    )
    with pytest.raises(ValidationError):
        snapshot.verify(Projection({}, {}, {}))


def test_reader_minimum_version_is_fixed() -> None:
    manifest = object_value(fixture("manifest.json"))
    manifest["min_reader_version"] = "0.9.0"
    with pytest.raises(ValidationError):
        read_snapshot(manifest, payloads())


def test_sorted_base_is_required_even_with_valid_new_hashes() -> None:

    manifest = object_value(fixture("manifest.json"))
    values = {key: parse(raw) for key, raw in payloads().items()}
    fragment = next(
        object_value(f)
        for key, raw in values.items()
        if key.startswith("bootstrap/")
        for f in array(object_value(object_value(raw)["tables"]).get("face", []))
        if len(array(object_value(f)["rows"])) > 1
    )
    fragment["rows"] = list(reversed(array(fragment["rows"])))
    with pytest.raises(ValueError, match=r"^Rows must be sorted with unique keys$"):
        read_snapshot(manifest, _reseal(manifest, values))


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("order", "Fragments are out of order"),
        ("duplicate", "Duplicate fragment identity"),
        ("base", "Base fragment identity mismatch"),
        ("missing_base", "Base fragment not found"),
    ],
)
def test_fragment_index_rejects_resealed_invalid_members(
    change: str, message: str
) -> None:
    manifest = object_value(fixture("manifest.json"))
    values = {key: parse(raw) for key, raw in payloads().items()}
    if change in {"base", "missing_base"}:
        base = next(
            object_value(object_value(fragment)["base"])
            for value in values.values()
            for entries in object_value(object_value(value).get("tables", {})).values()
            for fragment in array(entries)
            if object_value(fragment)["base"] is not None
        )
        target = base
        if change == "missing_base":
            # Moving the referenced bootstrap fragment leaves a consistent base unmatched.
            file = object_value(values[string(object_value(base["file"])["key"])])
            target = next(
                object_value(fragment)
                for fragment in array(
                    object_value(file["tables"])[string(base["table"])]
                )
                if all(
                    object_value(fragment)[field] == base[field]
                    for field in ("owner", "bucket")
                )
            )
        target["bucket"] = (integer(target["bucket"]) + 1) % 64
    else:
        entries = next(
            array(entries)
            for value in values.values()
            for entries in object_value(object_value(value).get("tables", {})).values()
            if len(array(entries)) > 1
        )
        if change == "order":
            entries.reverse()
        else:
            entries.append(entries[0])
    with pytest.raises(ValueError, match="^" + message + "$"):
        read_snapshot(manifest, _reseal(manifest, values))


@pytest.mark.parametrize("value", [".\n", "foo/..\n"])
def test_path_rejects_dot_segments_before_trailing_newline(value: str) -> None:
    with pytest.raises(ValidationError) as error:
        validate("Path", value)
    assert error.value.validator == "pattern"
    assert list(error.value.path) == []
