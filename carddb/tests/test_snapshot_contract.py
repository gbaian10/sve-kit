"""Consume shared, handwritten contract fixtures without a producer or database."""

import json
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator, ValidationError
from pydantic import JsonValue, TypeAdapter

from sve_carddb.snapshot.contract import definition, schema, tables, validate
from sve_carddb.snapshot.generate_schema import generate
from sve_carddb.snapshot.reader import read_snapshot, read_text_all
from sve_carddb.snapshot.values import (
    array,
    bucket,
    canonical,
    digest,
    object_value,
    parse,
    string,
)

FIXTURES = Path(__file__).resolve().parents[2] / "tests/fixtures/snapshot-contract/v1"
ADAPTER: TypeAdapter[JsonValue] = TypeAdapter(JsonValue)


def fixture(name: str) -> JsonValue:
    return ADAPTER.validate_python(json.loads((FIXTURES / name).read_bytes()))


def payloads() -> dict[str, bytes]:
    return {
        path.stem: canonical(fixture("payloads/" + path.name))
        for path in (FIXTURES / "payloads").glob("*.json")
    }


def test_schema_is_valid_and_covers_every_collection() -> None:
    Draft202012Validator.check_schema(schema())
    valid = [object_value(case) for case in array(fixture("schema-valid.json"))]
    assert len(tables()) == 43
    assert set(tables()) <= {string(case["schema"]) for case in valid}
    for case in valid:
        validate(string(case["schema"]), case["value"])


def test_schema_regeneration_matches_committed_bytes() -> None:
    resource = (
        Path(__file__).resolve().parents[1]
        / "src/sve_carddb/snapshot/schema/v1/contract.schema.json"
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
    for raw in array(fixture("schema-invalid.json")):
        case = object_value(raw)
        if "raw_json" in case:
            continue
        selected = schema() | {"$ref": "#/$defs/" + string(case["schema"])}
        selected.pop("oneOf")
        with pytest.raises(ValidationError):
            Draft202012Validator(selected).validate(case["value"])


@pytest.mark.parametrize(
    "case",
    array(fixture("schema-invalid.json")),
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


def test_text_all_matches_individual_downloads() -> None:
    blobs = payloads()
    attachments = {key: blobs[key] for key in ("images", "programs")}
    assert read_text_all(
        fixture("manifest.json"), canonical(fixture("text-all.json")), attachments
    ) == fixture("expected-logical.json")


def _replace(value: JsonValue, path: list[JsonValue], replacement: JsonValue) -> None:
    current = value
    for segment in path[:-1]:
        current = (
            array(current)[segment]
            if isinstance(segment, int)
            else object_value(current)[string(segment)]
        )
    last = path[-1]
    if isinstance(last, int):
        array(current)[last] = replacement
    else:
        object_value(current)[string(last)] = replacement


def _reseal(manifest: dict[str, JsonValue], key: str, data: bytes) -> None:
    file = next(
        object_value(item)
        for item in array(manifest["files"])
        if object_value(item)["key"] == key
    )
    hashed = digest(data)
    file.update(
        sha256=hashed, bytes=len(data), path="snapshots/blobs/" + hashed[7:] + ".json"
    )
    # Semantic mutations must pass the byte-integrity gate to exercise joins.
    if key in {"detail", "history"}:
        description = object_value(manifest["text_all"])
        for ref in array(description["contains"]):
            if object_value(ref)["key"] == key:
                object_value(ref)["sha256"] = hashed
    if key == "detail":
        decoded = object_value(parse(data))
        for count in array(file["row_counts"]):
            item = object_value(count)
            frags = array(object_value(decoded["tables"])[string(item["table"])])
            item["count"] = len(array(object_value(frags[0])["rows"]))


@pytest.mark.parametrize(
    "case",
    array(fixture("reader-invalid.json")),
    ids=lambda case: object_value(case)["name"],
)
def test_shared_reader_counterexamples(case: JsonValue) -> None:
    item = object_value(case)
    manifest = object_value(fixture("manifest.json"))
    blobs = payloads()
    target = string(item["target"])
    value = manifest if target == "manifest" else parse(blobs[target])
    _replace(value, array(item["path"]), item["value"])
    if target != "manifest":
        blobs[target] = canonical(value)
        if item["rehash"]:
            _reseal(manifest, target, blobs[target])
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
    ],
    ids=["duplicate", "float", "nan", "unsafe", "surrogate", "bom"],
)
def test_invalid_json_boundary(data: bytes) -> None:
    with pytest.raises((ValueError, UnicodeError)):
        parse(data)


def test_resources_available_from_package() -> None:
    assert schema()["$id"] == "urn:sve-kit:snapshot:1.0.0"
    assert len(canonical(schema())) < 1024 * 1024


def _replace_bootstrap(
    manifest: dict[str, JsonValue], blobs: dict[str, bytes], value: JsonValue
) -> None:
    blobs["bootstrap"] = canonical(value)
    _reseal(manifest, "bootstrap", blobs["bootstrap"])
    hashed = digest(blobs["bootstrap"])
    files = {
        string(object_value(item)["key"]): object_value(item)
        for item in array(manifest["files"])
    }
    files["detail"]["dependencies"] = [{"key": "bootstrap", "sha256": hashed}]
    detail = object_value(parse(blobs["detail"]))
    for table in ("printing", "face_revision"):
        fragment = object_value(array(object_value(detail["tables"])[table])[0])
        object_value(object_value(fragment["base"])["file"])["sha256"] = hashed
    for ref in array(object_value(manifest["text_all"])["contains"]):
        if object_value(ref)["key"] == "bootstrap":
            object_value(ref)["sha256"] = hashed
    blobs["detail"] = canonical(detail)
    _reseal(manifest, "detail", blobs["detail"])


def test_sorted_base_is_required_even_with_valid_new_hashes() -> None:
    manifest = object_value(fixture("manifest.json"))
    blobs = payloads()
    boot = object_value(parse(blobs["bootstrap"]))
    fragment = object_value(array(object_value(boot["tables"])["printing"])[0])
    fragment["rows"] = list(reversed(array(fragment["rows"])))
    _replace_bootstrap(manifest, blobs, boot)
    with pytest.raises(ValueError, match="Rows must be sorted"):
        read_snapshot(manifest, blobs)


def test_dangling_vocabulary_is_not_repaired() -> None:
    manifest = object_value(fixture("manifest.json"))
    blobs = payloads()
    boot = object_value(parse(blobs["bootstrap"]))
    fragment = object_value(array(object_value(boot["tables"])["face_revision"])[0])
    array(array(fragment["rows"])[0])[5] = "missing_type"
    _replace_bootstrap(manifest, blobs, boot)
    with pytest.raises(ValueError, match="Vocabulary reference missing"):
        read_snapshot(manifest, blobs)


def test_text_all_cannot_silently_replace_member_content() -> None:
    manifest = object_value(fixture("manifest.json"))
    union = object_value(fixture("text-all.json"))
    member = object_value(array(union["members"])[0])
    object_value(member["payload"])["format_version"] = "2.0.0"
    data = canonical(union)
    description = object_value(manifest["text_all"])
    hashed = digest(data)
    description.update(
        sha256=hashed, bytes=len(data), path="snapshots/blobs/" + hashed[7:] + ".json"
    )
    attachments = {key: payloads()[key] for key in ("images", "programs")}
    with pytest.raises(ValidationError):
        read_text_all(manifest, data, attachments)


def test_older_minimum_reader_version_is_compatible() -> None:
    manifest = object_value(fixture("manifest.json"))
    manifest["min_reader_version"] = "0.9.0"
    assert read_snapshot(manifest, payloads()) == fixture("expected-logical.json")
