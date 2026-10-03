"""Shared independent 2.0 vectors for Python and TypeScript consumers."""

import json
from copy import deepcopy
from pathlib import Path
from typing import TYPE_CHECKING, cast

import pytest
from jsonschema import ValidationError

from sve_carddb.snapshot.contract import validate
from sve_carddb.snapshot.media import image_url
from sve_carddb.snapshot.profiles import MEDIA
from sve_carddb.snapshot.reader import read_index, read_snapshot, select_index_entry
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    object_value,
    parse,
    string,
)

from .test_snapshot_contract import _replace

if TYPE_CHECKING:
    from pydantic import JsonValue

GOLDEN = Path(__file__).resolve().parents[2] / "tests/fixtures/snapshot-contract/v2"


def fixture(name: str) -> JsonValue:
    return parse((GOLDEN / name).read_bytes())


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

    def rebind(value: JsonValue) -> None:
        if isinstance(value, dict):
            if set(value) == {"key", "sha256"} and value["key"] in files:
                value["sha256"] = files[string(value["key"])]["sha256"]
            else:
                for child in value.values():
                    rebind(child)
        elif isinstance(value, list):
            for child in value:
                rebind(child)

    # Rebind exact bootstrap pins before sealing dependent text and media files.
    order = sorted(
        files,
        key=lambda k: (
            0
            if files[k]["role"] in {"config", "programs", "bootstrap"}
            else 2
            if files[k]["role"] == "images"
            else 1,
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
            {
                "table": table,
                **{f: fragment[f] for f in ("owner", "bucket", "partition")},
                "count": len(array(fragment["rows"])),
            }
            for table, entries in object_value(payload.get("tables", {})).items()
            for raw_fragment in array(entries)
            for fragment in (object_value(raw_fragment),)
        ]
        blobs[key] = raw
    rebind(manifest["config_ref"])
    rebind(manifest["text_all"])
    return blobs


@pytest.mark.parametrize(
    "case", array(fixture("reader-invalid.json")), ids=lambda c: object_value(c)["name"]
)
def test_shared_v2_reader_counterexamples(
    wire: tuple[dict[str, JsonValue], dict[str, JsonValue]], case: JsonValue
) -> None:
    manifest, payloads = deepcopy(wire)
    item = object_value(case)
    for raw in [*array(item.get("setup", [])), item]:
        mutation = object_value(raw)
        target = string(mutation["target"])
        _replace(
            manifest if target == "manifest" else payloads[target],
            array(mutation["path"]),
            mutation["value"],
        )
    blobs = _reseal(manifest, payloads)
    with pytest.raises(ValueError, match="^" + string(item["error"]) + "$"):
        read_snapshot(manifest, blobs)


@pytest.mark.parametrize(
    "case", array(fixture("schema-invalid.json")), ids=lambda c: object_value(c)["name"]
)
def test_shared_v2_schema_counterexamples(case: JsonValue) -> None:
    item = object_value(case)
    if "raw_json" in item:
        value = cast("JsonValue", json.loads(string(item["raw_json"])))
        with pytest.raises(ValueError, match="^" + string(item["error"]) + "$"):
            validate(string(item["target"]), value, MEDIA)
    else:
        with pytest.raises(ValidationError):
            validate(string(item["target"]), item["value"], MEDIA)


@pytest.mark.parametrize(
    "case",
    array(fixture("image-url-cases.json")),
    ids=lambda c: object_value(c)["case"],
)
def test_shared_v2_url_cases(case: JsonValue) -> None:
    item = object_value(case)
    args_value = (
        object_value(cast("JsonValue", json.loads(string(item["raw_json"]))))
        if "raw_json" in item
        else item
    )
    # Preserve invalid JSON scalar types at the public accessor boundary.
    args = (
        cast("int", args_value["int_id"]),
        cast("int", args_value["ordinal"]),
        string(args_value["size"]),
        cast("int", args_value["version"]),
    )
    if "reject" in item:
        with pytest.raises(ValueError, match="^" + string(item["reject"]) + "$"):
            image_url(*args)
    else:
        assert image_url(*args) == item["url"]


@pytest.mark.parametrize(
    "case", array(fixture("index-cases.json")), ids=lambda c: object_value(c)["name"]
)
def test_shared_v2_index_cases(case: JsonValue) -> None:
    item = object_value(case)
    index = object_value(item["index"])
    manifests = {k: canonical(v) for k, v in object_value(item["manifests"]).items()}
    if "error" in item:
        with pytest.raises(ValueError, match="^" + string(item["error"]) + "$"):
            read_index(index, manifests)
    else:
        assert read_index(index, manifests) == index
        selected = select_index_entry(index)
        assert (None if selected is None else selected["data_version"]) == item[
            "selected_data_version"
        ]
