"""Shared independent 3.0 vectors for Python and TypeScript consumers."""

import json
from copy import deepcopy
from pathlib import Path
from typing import TYPE_CHECKING, cast

import pytest
from jsonschema import ValidationError

from sve_carddb.contracts.profiles import MEDIA
from sve_carddb.contracts.snapshot import validate
from sve_carddb.core.json import array, canonical, object_value, parse, string
from sve_carddb.export.media_urls import image_url
from sve_carddb.export.reader import read_index, read_snapshot, select_index_entry

from ..support.snapshot_contract_fixtures import _replace, _reseal
from ..support.snapshot_contract_fixtures import wire as wire  # ruff: ignore[useless-import-alias] -- share one immutable golden input per module

if TYPE_CHECKING:
    from pydantic import JsonValue

GOLDEN = Path(__file__).resolve().parents[3] / "tests/fixtures/snapshot-contract/v3"


def fixture(name: str) -> JsonValue:
    return parse((GOLDEN / name).read_bytes())


@pytest.mark.parametrize(
    "case", array(fixture("reader-invalid.json")), ids=lambda c: object_value(c)["name"]
)
def test_shared_v3_reader_counterexamples(
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
def test_shared_v3_schema_counterexamples(case: JsonValue) -> None:
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
def test_shared_v3_url_cases(case: JsonValue) -> None:
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
def test_shared_v3_index_cases(case: JsonValue) -> None:
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


@pytest.mark.parametrize(
    "case",
    [
        item
        for item in array(fixture("schema-invalid.json"))
        if string(object_value(item)["name"]).startswith("Translation-")
    ],
    ids=lambda c: object_value(c)["name"],
)
def test_current_translation_shape_rejected_inside_resealed_wire(
    wire: tuple[dict[str, JsonValue], dict[str, JsonValue]], case: JsonValue
) -> None:
    manifest, payloads = deepcopy(wire)
    payloads["bootstrap/bootstrap/global/global/band/2"] = object_value(case)["value"]
    blobs = _reseal(manifest, payloads)
    with pytest.raises(ValidationError):
        read_snapshot(manifest, blobs)
