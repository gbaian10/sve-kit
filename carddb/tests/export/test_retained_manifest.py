"""Retained publication metadata is independent of current card data schemas."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.profiles import MEDIA, profile
from sve_carddb.export.read_api import (
    ExportError,
    array,
    canonical,
    closure,
    digest,
    object_value,
    retained_manifest,
    string,
)

if TYPE_CHECKING:
    from pydantic import JsonValue


def envelope() -> dict[str, JsonValue]:
    """Only publication fields are stable across card data schema revisions."""
    return {
        "data_version": "preview-20261019T010203Z-0001",
        "published_at": "2026-10-19T01:02:03Z",
        "format_version": MEDIA,
        "min_reader_version": "4.0.0",
        "required_capabilities": list(profile(MEDIA).capabilities),
        "engine_support_target": {
            "engine_version": None,
            "engine_build_hash": None,
            "validation_policy_id": None,
        },
        "files": [
            {
                "path": "snapshots/blobs/" + "a" * 64 + ".json",
                "sha256": "sha256:" + "a" * 64,
                "bytes": 12,
                "compressed_bytes": {"br": 8},
            }
        ],
        "changes_ref": None,
        "errata_card_ids": [],
    }


def entry(value: dict[str, JsonValue], raw: bytes) -> dict[str, JsonValue]:
    return {
        key: value[key]
        for key in (
            "data_version",
            "published_at",
            "format_version",
            "min_reader_version",
            "required_capabilities",
            "engine_support_target",
        )
    } | {
        "manifest_sha256": digest(raw),
        "manifest_path": "snapshots/manifests/" + digest(raw)[7:] + ".json",
    }


def test_retained_manifest_uses_only_the_publication_envelope() -> None:
    value = envelope()
    raw = canonical(value)
    identity = entry(value, raw)
    result = retained_manifest(raw, identity)
    assert result == value
    assert closure(string(identity["manifest_path"]), result) == {
        identity["manifest_path"],
        "snapshots/blobs/" + "a" * 64 + ".json",
        "snapshots/blobs/" + "a" * 64 + ".json.br",
    }


@pytest.mark.parametrize(
    ("description", "damage", "error", "message"),
    [
        ("files", "invalid", TypeError, "Expected array"),
        ("files", [None], TypeError, "Expected object"),
        ("files", {}, TypeError, "Expected array"),
        ("changes_ref", "invalid", TypeError, "Expected object"),
        ("changes_ref", [None], TypeError, "Expected object"),
        ("changes_ref", {}, KeyError, "path"),
    ],
)
def test_retained_manifest_rejects_invalid_file_lists(
    description: str, damage: JsonValue, error: type[Exception], message: str
) -> None:
    value = envelope()
    value[description] = damage
    raw = canonical(value)
    with pytest.raises(error, match=message):
        retained_manifest(raw, entry(value, raw))


@pytest.mark.parametrize(
    ("field", "damage", "error", "message"),
    [
        ("path", "../private.json", ExportError, "Invalid immutable payload path"),
        (
            "path",
            "snapshots/blobs/" + "b" * 64 + ".json",
            ExportError,
            "Invalid immutable payload path",
        ),
        ("bytes", 0, ExportError, "Invalid retained manifest file description"),
        ("bytes", -1, ExportError, "Invalid retained manifest file description"),
        ("bytes", True, ValueError, "Expected safe integer"),
        ("bytes", "12", ValueError, "Expected safe integer"),
        ("compressed_bytes", {}, KeyError, "br"),
        (
            "compressed_bytes",
            {"br": 0},
            ExportError,
            "Invalid retained manifest file description",
        ),
        (
            "compressed_bytes",
            {"br": -1},
            ExportError,
            "Invalid retained manifest file description",
        ),
        ("compressed_bytes", {"br": True}, ValueError, "Expected safe integer"),
        ("compressed_bytes", {"br": "8"}, ValueError, "Expected safe integer"),
    ],
)
@pytest.mark.parametrize("description", ["files", "changes_ref"])
def test_retained_manifest_rejects_bad_file_descriptions(
    field: str,
    damage: JsonValue,
    error: type[Exception],
    message: str,
    description: str,
) -> None:
    value = envelope()
    row = dict(object_value(array(value["files"])[0]))
    row[field] = damage
    value[description] = [row] if description == "files" else row
    raw = canonical(value)
    with pytest.raises(error, match=message):
        retained_manifest(raw, entry(value, raw))


@pytest.mark.parametrize("description", ["files", "changes_ref"])
def test_retained_manifest_rejects_invalid_hash_with_matching_path(
    description: str,
) -> None:
    value = envelope()
    row = dict(object_value(array(value["files"])[0]))
    row["sha256"] = "sha256:" + "A" * 64
    row["path"] = "snapshots/blobs/" + "A" * 64 + ".json"
    value[description] = [row] if description == "files" else row
    raw = canonical(value)
    with pytest.raises(ExportError, match="Invalid retained manifest file description"):
        retained_manifest(raw, entry(value, raw))


@pytest.mark.parametrize(
    ("damage", "message"),
    [
        ("hash", "Retained manifests must be intact"),
        ("canonical", "Manifest differs from its index entry"),
        ("version", "Manifest differs from its index entry"),
        ("published", "Manifest differs from its index entry"),
        ("path", "Manifest differs from its index entry"),
        ("format", "Unsupported retained snapshot format"),
    ],
)
def test_retained_manifest_rejects_damaged_identity(damage: str, message: str) -> None:
    value = envelope()
    if damage == "format":
        value["format_version"] = "5.0.0"
    raw = canonical(value)
    if damage == "canonical":
        raw += b"\n"
    identity = entry(value, raw)
    if damage == "hash":
        raw += b" "
    elif damage == "version":
        identity["data_version"] = "preview-20261018T010203Z-0001"
    elif damage == "published":
        identity["published_at"] = "2026-10-18T01:02:03Z"
    elif damage == "path":
        identity["manifest_path"] = "snapshots/manifests/" + "b" * 64 + ".json"
    with pytest.raises(ExportError, match=message):
        retained_manifest(raw, identity)
