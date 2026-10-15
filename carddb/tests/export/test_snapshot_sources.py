"""Exact source presentation and local dictionary rejection through the full reader."""

from copy import deepcopy

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.snapshot import columns, decode, definition
from sve_carddb.core.json import array, canonical, object_value, string
from sve_carddb.export.project import Projection
from sve_carddb.export.reader import read_snapshot
from sve_carddb.export.semantics import row_key
from sve_carddb.export.transport import Batch, Ownership, export_snapshot
from sve_carddb.export.transport.wire import encode

from ..support.snapshot_contract_fixtures import _reseal, fixture


def source_input() -> tuple[dict[str, JsonValue], dict[str, JsonValue]]:
    value = object_value(fixture("source-native.json"))
    return deepcopy(object_value(value["manifest"])), deepcopy(
        object_value(value["payloads"])
    )


def test_sources_include_counterpart_and_each_owners_own_annotations() -> None:
    value = object_value(fixture("source-native.json"))
    manifest, payloads = source_input()
    view = read_snapshot(manifest, {k: canonical(v) for k, v in payloads.items()})
    assert view == value["expected"]
    files = {
        string(object_value(f)["key"]): object_value(f)
        for f in array(manifest["files"])
    }
    source_files = {
        string(object_value(ref)["key"])
        for ref in array(files["detail/en/family-BP02/0"]["source_refs"])
    }
    assert source_files == {"catalog/jp/family-BP02/0", "detail/jp/family-BP02/0"}
    assert "detail/en/family-BP02/0" in {
        string(object_value(ref)["key"])
        for ref in array(files["detail/jp/family-BP02/0"]["source_refs"])
    }
    assert len(view["annotation_set"]) == 4


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("missing_local_dictionary", "Missing file-local dictionary reference"),
        ("conflicting_copy", "Conflicting dictionary copy"),
        ("bad_pointer", "public-annotation/owner"),
        ("missing_source", "Source presentation closure mismatch"),
        ("ordinary_reference", "Source presentation closure mismatch"),
        ("bad_hash", "Source reference hash mismatch"),
        ("bad_scope", "Base region scope mismatch"),
    ],
)
def test_source_and_dictionary_corruption_is_rejected(
    mutation: str, message: str
) -> None:
    manifest, payloads = source_input()
    file = next(
        object_value(f)
        for f in array(manifest["files"])
        if object_value(f)["key"] == "detail/en/family-BP02/0"
    )
    value = object_value(payloads["detail/en/family-BP02/0"])
    tables = object_value(value["tables"])
    if mutation in {"missing_local_dictionary", "conflicting_copy"}:
        fragment = array(array(tables["text_unit"])[0])
        rows = array(fragment[4])
        if mutation == "missing_local_dictionary":
            rows.clear()
        else:
            array(rows[0])[columns("text_unit").index("text")] = (
                "Different synthetic text"
            )
    elif mutation == "bad_pointer":
        fragment = array(array(tables["face_revision"])[0])
        row = decode("face_revision_detail", array(fragment[4])[0])
        selection = object_value(array(row["translations"])[0])
        object_value(object_value(selection["source"])["owner"])["id"] = (
            "missing-source-owner"
        )
        array(fragment[4])[0] = encode("face_revision_detail", row)
    elif mutation == "missing_source":
        file["source_refs"] = []
    elif mutation == "ordinary_reference":
        # Another printing's detail is an ordinary foreign key, not source presentation.
        printing = next(
            object_value(f)
            for f in array(manifest["files"])
            if object_value(f)["key"] == "print-detail/jp/family-BP02/0"
        )
        array(file["source_refs"]).append(
            {"key": printing["key"], "sha256": printing["sha256"]}
        )
    elif mutation == "bad_hash":
        object_value(array(file["source_refs"])[0])["sha256"] = "sha256:" + "0" * 64
    else:
        target = next(
            object_value(f)
            for f in array(manifest["files"])
            if object_value(f)["key"] == "detail/jp/family-BP02/0"
        )
        target["region"] = "en"
    with pytest.raises(ValueError, match=message):
        read_snapshot(manifest, _reseal(manifest, payloads))


def test_historical_source_does_not_require_current_source_body() -> None:
    value = object_value(fixture("source-native.json"))
    view = {
        t: [object_value(r) for r in array(rows)]
        for t, rows in object_value(deepcopy(value["expected"])).items()
    }
    source = object_value(object_value(value["source"])["owner"])
    original = next(r for r in view["face_revision"] if r["id"] == source["id"])
    historical = deepcopy(original) | {"id": "historical-source", "translations": []}
    view["face_revision"].append(historical)
    for row in list(view["field_annotation"]):
        if row["owner"] == source:
            view["field_annotation"].append(
                deepcopy(row) | {"owner": source | {"id": historical["id"]}}
            )
    receiver = object_value(object_value(value["receiver"])["owner"])
    row = next(r for r in view["face_revision"] if r["id"] == receiver["id"])
    object_value(object_value(array(row["translations"])[0])["source"])["owner"] = (
        source | {"id": historical["id"]}
    )
    projection, ownership = projection_input(view, value)
    result = export_snapshot(
        projection,
        ownership,
        Batch(
            "preview-20261015T000000Z-0001",
            "2026-10-15T00:00:00Z",
            "2026-10-15",
            ("en", "jp"),
        ),
    )
    file = next(
        object_value(f)
        for f in array(result.manifest["files"])
        if object_value(f)["key"] == "detail/en/family-BP02/0"
    )
    assert {string(object_value(ref)["key"]) for ref in array(file["source_refs"])} == {
        "catalog/jp/family-BP02/0",
        "history/jp/family-BP02/0",
    }


def projection_input(
    view: dict[str, list[dict[str, JsonValue]]], value: dict[str, JsonValue]
) -> tuple[Projection, Ownership]:
    config = object_value(object_value(value["payloads"])["config"]).copy()
    config.pop("format_version")
    manifest = object_value(value["manifest"])
    metadata = {
        k: v
        for k, v in manifest.items()
        if k
        not in {
            "files",
            "config_ref",
            "partitioning",
            "changes_ref",
            "format_version",
            "data_version",
            "published_at",
            "data_as_of",
            "regions",
            "languages",
            "min_reader_version",
            "required_capabilities",
        }
    }
    homes = {
        string(array(r)[0]): string(array(array(f)[0])[1])
        for payload in object_value(value["payloads"]).values()
        for f in array(
            object_value(object_value(payload).get("tables", {})).get("printing", [])
        )
        if array(f)[3] is None
        for r in array(array(f)[4])
    }
    for table, rows in view.items():
        rows.sort(
            key=lambda r: tuple(
                row_key(r[string(k)]) for k in array(definition(table)["x-primary-key"])
            )
        )
    return Projection(view, config, metadata), Ownership(homes)


def test_shared_translation_position_is_fixed_before_english_uses_it() -> None:
    value = object_value(fixture("source-native.json"))
    view = {
        t: [object_value(r) for r in array(rows)]
        for t, rows in object_value(deepcopy(value["expected"])).items()
    }
    receiver = object_value(object_value(value["receiver"])["owner"])
    next(r for r in view["face_revision"] if r["id"] == receiver["id"])[
        "translations"
    ] = []
    projection, ownership = projection_input(view, value)
    snapshot = export_snapshot(
        projection,
        ownership,
        Batch(
            "preview-20261015T000000Z-0001",
            "2026-10-15T00:00:00Z",
            "2026-10-15",
            ("en", "jp"),
        ),
    )
    original = object_value(value["payloads"])
    assert "translation/shared/family-BP02/0" in snapshot.payloads
    for key, payload in snapshot.payloads.items():
        if "/jp/" in key or key.startswith("translation/"):
            assert payload.raw == canonical(original[key])


def test_printed_source_uses_its_permanent_printing_home() -> None:
    value = object_value(fixture("source-native.json"))
    view = {
        t: [object_value(r) for r in array(rows)]
        for t, rows in object_value(deepcopy(value["expected"])).items()
    }
    source = object_value(object_value(value["source"])["owner"])
    revision = next(r for r in view["face_revision"] if r["id"] == source["id"])
    printing = next(
        p
        for p in view["printing"]
        if p["region"] == "jp"
        and any(
            object_value(f)["face_id"] == revision["face_id"] for f in array(p["faces"])
        )
    )
    face = next(
        object_value(f)
        for f in array(printing["faces"])
        if object_value(f)["face_id"] == revision["face_id"]
    )
    face.update(
        printed_name_unit_id=revision["name_unit_id"],
        printed_effect_unit_id=revision["effect_unit_id"],
        printed_text_state="verified",
        sections=deepcopy(revision["sections"]),
    )
    owner: dict[str, JsonValue] = {
        "kind": "printing_face",
        "id": printing["id"],
        "face_id": revision["face_id"],
    }
    for row in list(view["field_annotation"]):
        if row["owner"] == source:
            view["field_annotation"].append(deepcopy(row) | {"owner": owner})
    row = next(
        r
        for r in view["face_revision"]
        if r["id"] == object_value(object_value(value["receiver"])["owner"])["id"]
    )
    selection = object_value(array(row["translations"])[0])
    object_value(selection["source"])["owner"] = owner
    row["translations"] = []
    english = next(
        p
        for p in view["printing"]
        if p["region"] == "en" and p["card_id"] == printing["card_id"]
    )
    receiver_face = next(
        object_value(f)
        for f in array(english["faces"])
        if object_value(f)["face_id"] == revision["face_id"]
    )
    receiver_face.update(
        printed_name_unit_id=row["name_unit_id"],
        printed_effect_unit_id=row["effect_unit_id"],
        printed_text_state="verified",
        translations=[selection],
    )
    projection, ownership = projection_input(view, value)
    snapshot = export_snapshot(
        projection,
        Ownership(dict(ownership.printing_home) | {string(printing["id"]): "GF01"}),
        Batch(
            "preview-20261015T000000Z-0001",
            "2026-10-15T00:00:00Z",
            "2026-10-15",
            ("en", "jp"),
        ),
    )
    file = next(
        object_value(f)
        for f in array(snapshot.manifest["files"])
        if object_value(f)["key"] == "print-detail/en/family-BP02/0"
    )
    assert {string(object_value(ref)["key"]) for ref in array(file["source_refs"])} == {
        "catalog/jp/family-BP02/0",
        "catalog/jp/family-GF01/0",
        "print-detail/jp/family-GF01/0",
    }
