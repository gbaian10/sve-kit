"""Pending name agreement does not adopt any candidate's complete wording."""

from copy import deepcopy

import pytest

from sve_carddb.core.json import array, object_value, string
from sve_carddb.export.common_names import Row, common_name
from sve_carddb.export.project import Projection
from sve_carddb.export.reader import read_snapshot
from sve_carddb.export.transport import Ownership, export_snapshot

from ..support.snapshot_contract_fixtures import fixture
from .test_snapshot_export import BATCH
from .test_snapshot_wording import pending_view


def test_common_name_is_available_without_candidate_history() -> None:
    view, manifest, _ = pending_view()
    face = view["face"][0]
    wording = object_value(array(face["wording"])[0])
    wording["display"] = {"revision_id": None, "basis": "candidates"}
    wording["candidates"] = [
        {"printing_id": "p:a", "revision_id": "r:a2"},
        {"printing_id": "p:b", "revision_id": "r:a1"},
    ]
    printed = object_value(array(view["printing"][1]["faces"])[0])
    printed["observations"] = [
        {
            "revision_id": "r:a1",
            "state": "available",
            "source_url": "https://example.test/b",
        }
    ]
    revisions = {string(row["id"]): row for row in view["face_revision"]}
    selection = deepcopy(
        next(
            object_value(raw)
            for raw in array(revisions["r:a2"]["translations"])
            if object_value(raw)["field"] == "name"
        )
    )
    object_value(object_value(selection["source"])["owner"])["id"] = "r:a1"
    revisions["r:a1"]["translations"] = [selection]
    wording["common_name"] = common_name(wording, revisions)
    assert wording["common_name"] == {
        "name_unit_id": revisions["r:a1"]["name_unit_id"],
        "translations": [
            {
                "field": "name",
                "ordinal": None,
                "target_lang": "zh-Hant",
                "translation_id": "tr:name",
                "basis": "own_source",
            }
        ],
    }
    projection = Projection(
        view, object_value(fixture("payloads/config.json")), manifest
    )
    result = export_snapshot(
        projection,
        Ownership({"p:a": "set:a", "p:b": "set:a"}),
        BATCH,
    )
    result.verify(Projection(view, {}, {}))
    restored = read_snapshot(
        result.manifest, {key: blob.raw for key, blob in result.payloads.items()}
    )
    assert restored["face"][0]["current"] == []
    assert (
        object_value(array(restored["face"][0]["wording"])[0])["common_name"]
        == wording["common_name"]
    )
    catalog = [
        blob for key, blob in result.payloads.items() if key.startswith("catalog/")
    ]
    assert any(b"tr:name" in blob.raw for blob in catalog)
    assert all(
        not string(object_value(dependency)["key"]).startswith("history/")
        for file in array(result.manifest["files"])
        if object_value(file)["purpose"] == "catalog"
        for dependency in array(object_value(file)["dependencies"])
    )
    wording["common_name"] = None
    with pytest.raises(ValueError, match="Pending common name differs"):
        export_snapshot(projection, Ownership({"p:a": "set:a", "p:b": "set:a"}), BATCH)


@pytest.mark.parametrize(
    "change", ["different", "missing", "missing_translation", "display"]
)
def test_common_name_respects_disagreement_and_missing_candidates(change: str) -> None:
    selection: Row = {
        "field": "name",
        "ordinal": None,
        "target_lang": "zh-Hant",
        "translation_id": "tr:name",
        "basis": "own_source",
    }
    revisions: dict[str, Row] = {
        "r:a": {"name_unit_id": "t:name", "translations": [selection]},
        "r:b": {"name_unit_id": "t:name", "translations": [selection]},
    }
    wording: Row = {
        "display": {"revision_id": None},
        "candidates": [{"revision_id": "r:a"}, {"revision_id": "r:b"}],
    }
    if change == "different":
        revisions["r:b"]["name_unit_id"] = "t:other"
    elif change == "missing":
        object_value(array(wording["candidates"])[1])["revision_id"] = None
    elif change == "missing_translation":
        revisions["r:b"]["translations"] = []
    else:
        object_value(wording["display"])["revision_id"] = "r:a"
    result = common_name(wording, revisions)
    assert result == (
        {"name_unit_id": "t:name", "translations": []}
        if change == "missing_translation"
        else None
    )
