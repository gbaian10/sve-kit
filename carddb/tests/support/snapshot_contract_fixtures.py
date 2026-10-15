"""Shared independent 4.0 golden inputs and deliberate resealing for counterexamples."""

import json
from pathlib import Path

import pytest
from pydantic import JsonValue, TypeAdapter

from sve_carddb.core.json import array, canonical, digest, object_value, parse, string

GOLDEN = Path(__file__).resolve().parents[3] / "tests/fixtures/snapshot-contract/v3"
ADAPTER: TypeAdapter[JsonValue] = TypeAdapter(JsonValue)


def fixture(name: str) -> JsonValue:
    if name.startswith("payloads/") and not (GOLDEN / name).exists():
        key = Path(name).stem
        manifest = object_value(fixture("manifest.json"))
        file = next(
            object_value(f)
            for f in array(manifest["files"])
            if object_value(f)["key"] == key
        )
        name = "payloads/" + Path(string(file["path"])).name
    return ADAPTER.validate_python(json.loads((GOLDEN / name).read_bytes()))


def payloads() -> dict[str, bytes]:
    return {
        string(file["key"]): canonical(
            fixture("payloads/" + Path(string(file["path"])).name)
        )
        for raw in array(object_value(fixture("manifest.json"))["files"])
        for file in (object_value(raw),)
    }


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


@pytest.fixture(scope="module")
def wire() -> tuple[dict[str, JsonValue], dict[str, JsonValue]]:
    manifest = object_value(fixture("manifest.json"))
    payloads = {
        string(f["key"]): parse(
            (GOLDEN / "payloads" / Path(string(f["path"])).name).read_bytes()
        )
        for raw in array(manifest["files"])
        for f in (object_value(raw),)
    }
    return manifest, payloads


def _reseal(
    manifest: dict[str, JsonValue], payloads: dict[str, JsonValue]
) -> dict[str, bytes]:
    files = {
        string(f["key"]): f
        for raw in array(manifest["files"])
        for f in (object_value(raw),)
    }

    old_hashes = {key: file["sha256"] for key, file in files.items()}

    def rebind(value: JsonValue) -> None:
        if isinstance(value, dict):
            if (
                set(value) == {"key", "sha256"}
                and value["key"] in files
                and value["sha256"] == old_hashes[string(value["key"])]
            ):
                value["sha256"] = files[string(value["key"])]["sha256"]
            else:
                for child in value.values():
                    rebind(child)
        elif isinstance(value, list):
            for child in value:
                rebind(child)

    # Rebind exact bootstrap pins before sealing dependent text and media files.
    priority = {
        "config": 0,
        "programs": 0,
        "catalog": 1,
        "print": 3,
        "region-state": 3,
        "images": 5,
    }
    order = sorted(
        files,
        key=lambda k: (
            priority.get(string(files[k]["purpose"]), 4),
            0 if files[k]["region"] is None else 1,
            k,
        ),
    )
    blobs: dict[str, bytes] = {}
    for key in order:
        payload = object_value(payloads[key])
        rebind(payload)
        rebind(files[key]["dependencies"])
        raw = canonical(payload)
        hashed = digest(raw)
        files[key].update(
            sha256=hashed,
            bytes=len(raw),
            path="snapshots/blobs/" + hashed[7:] + ".json",
        )
        files[key]["row_counts"] = [
            [
                table,
                array(fragment[0])[0],
                array(fragment[0])[1],
                fragment[1],
                fragment[2],
                len(array(fragment[4])),
            ]
            for table, entries in object_value(payload.get("tables", {})).items()
            for raw_fragment in array(entries)
            for fragment in (array(raw_fragment),)
        ]
        blobs[key] = raw
    for file in files.values():
        rebind(file["dependencies"])
        rebind(file["source_refs"])
    rebind(manifest["config_ref"])
    return blobs
