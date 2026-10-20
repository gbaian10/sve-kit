"""Retained publication metadata is independent of current card data schemas."""

from typing import TYPE_CHECKING

import pytest
from jsonschema import ValidationError

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


@pytest.mark.parametrize("description", ["files", "changes_ref"])
@pytest.mark.parametrize("damage", ["invalid", [None], {}])
def test_retained_manifest_rejects_invalid_file_lists(
    description: str, damage: JsonValue
) -> None:
    value = envelope()
    value[description] = damage
    raw = canonical(value)
    with pytest.raises((ExportError, ValueError, TypeError, KeyError)):
        retained_manifest(raw, entry(value, raw))


@pytest.mark.parametrize(
    ("field", "damage"),
    [
        ("path", "../private.json"),
        ("path", "snapshots/blobs/" + "b" * 64 + ".json"),
        ("sha256", "wrong:" + "a" * 64),
        ("bytes", 0),
        ("bytes", -1),
        ("bytes", True),
        ("bytes", "12"),
        ("compressed_bytes", {}),
        ("compressed_bytes", {"br": 0}),
        ("compressed_bytes", {"br": -1}),
        ("compressed_bytes", {"br": True}),
        ("compressed_bytes", {"br": "8"}),
    ],
)
@pytest.mark.parametrize("description", ["files", "changes_ref"])
def test_retained_manifest_rejects_bad_file_descriptions(
    field: str, damage: JsonValue, description: str
) -> None:
    value = envelope()
    row = dict(object_value(array(value["files"])[0]))
    row[field] = damage
    value[description] = [row] if description == "files" else row
    raw = canonical(value)
    with pytest.raises((ExportError, ValueError, TypeError, KeyError, ValidationError)):
        retained_manifest(raw, entry(value, raw))


@pytest.mark.parametrize(
    "damage", ["hash", "canonical", "version", "published", "path"]
)
def test_retained_manifest_rejects_damaged_identity(damage: str) -> None:
    value = envelope()
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
    with pytest.raises(ExportError):
        retained_manifest(raw, identity)
