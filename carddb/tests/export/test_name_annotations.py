"""Whole-name wire omission preserves exact public annotation identities."""

from copy import deepcopy
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build import create_database
from sve_carddb.core.json import array, canonical, digest, object_value, parse, string
from sve_carddb.domains.translations.names.annotations import annotate_name
from sve_carddb.export.media import prepare_media
from sve_carddb.export.name_annotations import compact_names, whole_name
from sve_carddb.export.preview import Roots, write_preview
from sve_carddb.export.read_api import load_export
from sve_carddb.export.reader import read_snapshot, read_text_all
from sve_carddb.export.reader_annotations import validate_annotations
from sve_carddb.export.transport import Batch, Ownership, export_snapshot

from ..support.snapshot_project_fixtures import TEXT, populate, schema
from .test_snapshot_project import projected

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.export.media import MediaPlan
    from sve_carddb.export.project import Projection
    from sve_carddb.export.transport import Snapshot


def name_snapshot() -> tuple[Projection, Snapshot, MediaPlan]:
    """Use resolved source occurrences from a synthetic native build."""
    with create_database(schema(with_annotations=True)) as db:
        with db.transaction():
            populate(db)
            db.update("glossary_term", {"id": "term"}, {"category": "card_name"})
            annotate_name(db, "use", TEXT, "term")
        projection = projected(db)
        unit = string(projection.tables["translation"][0]["text_unit_id"])
        with db.transaction():
            # The target text is interned by the projection rather than stored in this fixture DB.
            text = next(
                row for row in projection.tables["text_unit"] if row["id"] == unit
            )
            db.insert(
                "text_unit",
                {
                    "id": unit,
                    "lang": string(text["lang"]),
                    "text": string(text["text"]),
                    "content_hash": digest(string(text["text"]).encode()),
                },
            )
            annotate_name(db, "translation", unit, "term", translated=True)
        projection = projected(db)
        projection = replace(
            projection,
            tables=projection.tables
            | {
                "printing_image": [
                    row | {"availability": "unfetched", "publication_state": "pending"}
                    for row in projection.tables["printing_image"]
                ],
                "image_variant": [],
            },
        )
        plan = prepare_media(projection, None, revision=1)
        projection = plan.projection
        ownership = Ownership.from_database(db, projection)
        snapshot = export_snapshot(
            projection,
            ownership,
            Batch("preview-20261011T010203Z-0001", "2026-10-11T01:02:03Z", ("jp",)),
        )
        return projection, snapshot, plan


def test_native_whole_name_round_trip_and_publish_boundary(tmp_path: Path) -> None:
    projection, snapshot, plan = name_snapshot()
    assert projection.tables["face_revision"][0]["name_concept_id"] == "term"
    translation = projection.tables["translation"][0]
    assert translation["annotation_kind"] == "whole_name"
    target = next(
        row
        for row in projection.tables["text_unit"]
        if row["id"] == translation["text_unit_id"]
    )
    assert translation["annotation_set_id"] == whole_name(target, "term")["id"]
    compact = compact_names(projection.tables)
    assert compact["annotation_set"] == compact["field_annotation"] == []
    assert compact["translation"][0]["annotation_set_id"] is None
    payloads = {key: blob.raw for key, blob in snapshot.payloads.items()}
    assert read_snapshot(snapshot.manifest, payloads) == projection.tables
    assert (
        read_text_all(
            snapshot.manifest,
            snapshot.text_all.raw,
            {
                key: raw
                for key, raw in payloads.items()
                if key == "programs" or key.startswith("images/")
            },
        )
        == projection.tables
    )
    for blob in snapshot.payloads.values():
        value = object_value(parse(blob.raw))
        for fragments in object_value(value.get("tables", {})).values():
            assert all(
                "columns" not in object_value(fragment) for fragment in array(fragments)
            )
    roots = Roots(tmp_path / "public", tmp_path / "private")
    write_preview(snapshot, roots, {}, media_plan=plan)
    load_export(roots.preview)


@pytest.mark.parametrize(
    "mutation",
    [
        "effect",
        "missing_source",
        "missing_concept",
        "different_concept",
        "unreferenced_translation",
    ],
)
def test_derived_translation_rejects_inexact_source(mutation: str) -> None:
    projection, _, _ = name_snapshot()
    view = compact_names(projection.tables)
    selection = object_value(array(view["face_revision"][0]["translations"])[0])
    if mutation == "effect":
        selection["field"] = object_value(selection["source"])["field"] = "effect"
    elif mutation == "missing_source":
        object_value(object_value(selection["source"])["owner"])["id"] = "missing"
    elif mutation == "missing_concept":
        view["face_revision"][0]["name_concept_id"] = None
    elif mutation == "different_concept":
        duplicate = deepcopy(view["face_revision"][0])
        duplicate.update(id="another-revision", name_concept_id="another-concept")
        view["face_revision"].append(duplicate)
    else:
        view["face_revision"][0]["translations"] = []
    with pytest.raises(ValueError, match="public-annotation/"):
        validate_annotations(view, ("ja", "zh-Hant"))


def test_multiple_occurrences_and_effect_sets_remain_explicit() -> None:
    projection, _, _ = name_snapshot()
    view = deepcopy(projection.tables)
    revision = view["face_revision"][0]
    unit = string(revision["effect_unit_id"])
    occurrences: list[JsonValue] = [
        {
            "ordinal": ordinal,
            "reference": {"kind": "card_name", "term_id": "term"},
            "ranges": [{"start": start, "end": end}],
            "bold": True,
        }
        for ordinal, (start, end) in enumerate(((0, 1), (2, 3)))
    ]
    identifier = (
        "ann:"
        + digest(
            canonical(
                {
                    "recipe": "annotation-v1",
                    "text_unit_id": unit,
                    "occurrences": occurrences,
                }
            )
        )[7:]
    )
    annotation: dict[str, JsonValue] = {
        "id": identifier,
        "text_unit_id": unit,
        "occurrences": occurrences,
    }
    view["annotation_set"].append(annotation)
    field: dict[str, JsonValue] = {
        "owner": {"kind": "face_revision", "id": revision["id"]},
        "field": "effect",
        "ordinal": None,
        "annotation_set_id": identifier,
    }
    view["field_annotation"].append(field)
    compact = compact_names(view)
    assert compact["annotation_set"] == [annotation]
    assert compact["field_annotation"] == [field]
    validate_annotations(compact, ("ja", "zh-Hant"))
    assert sorted(map(canonical, compact["annotation_set"])) == sorted(
        map(canonical, view["annotation_set"])
    )


def test_historical_and_printed_names_use_their_own_concept() -> None:
    projection, _, _ = name_snapshot()
    view = compact_names(projection.tables)
    historical = deepcopy(view["face_revision"][0])
    historical.update(id="historical", name_concept_id="historic-term", translations=[])
    view["face_revision"].append(historical)
    printed = object_value(array(view["printing"][0]["faces"])[0])
    printed.update(printed_name_unit_id=TEXT, name_concept_id="printed-term")
    for identifier in ("historic-term", "printed-term"):
        view["annotation_concept"].append(
            {
                "id": identifier,
                "category": "card_name",
                "card_ids": [],
                "explanations": [],
            }
        )
    validate_annotations(view, ("ja", "zh-Hant"))
    annotations = {row["id"]: row for row in view["annotation_set"]}
    references = {
        canonical(row["owner"]): object_value(
            array(annotations[row["annotation_set_id"]]["occurrences"])[0]
        )["reference"]
        for row in view["field_annotation"]
    }
    assert references[canonical({"kind": "face_revision", "id": "historical"})] == {
        "kind": "card_name",
        "term_id": "historic-term",
    }
    assert references[
        canonical({"kind": "printing_face", "id": "printing", "face_id": "face"})
    ] == {"kind": "card_name", "term_id": "printed-term"}


def test_jp_name_source_does_not_require_a_receiver_concept() -> None:
    projection, _, _ = name_snapshot()
    view = compact_names(projection.tables)
    for region in array(view["card"][0]["regions"]):
        object_value(region)["mapping_state"] = "confirmed"
    source = view["face_revision"][0]
    receiver = deepcopy(source)
    receiver.update(id="english-revision", region="en", name_concept_id=None)
    source["translations"] = []
    selection = object_value(array(receiver["translations"])[0])
    selection["basis"] = "jp_source"
    view["face_revision"].append(receiver)
    validate_annotations(view, ("ja", "zh-Hant"))
    assert (
        view["translation"][0]["annotation_set_id"]
        == projection.tables["translation"][0]["annotation_set_id"]
    )


def test_own_name_source_cannot_borrow_another_revision() -> None:
    projection, _, _ = name_snapshot()
    view = compact_names(projection.tables)
    historical = deepcopy(view["face_revision"][0])
    historical.update(id="historical", translations=[])
    view["face_revision"].append(historical)
    selection = object_value(array(view["face_revision"][0]["translations"])[0])
    object_value(object_value(selection["source"])["owner"])["id"] = "historical"
    with pytest.raises(ValueError, match="public-annotation/owner"):
        validate_annotations(view, ("ja", "zh-Hant"))


def test_counterpart_null_wire_slots_do_not_hide_different_concepts() -> None:
    projection, _, _ = name_snapshot()
    view = compact_names(projection.tables)
    translation = view["translation"][0]
    target = next(
        row for row in view["text_unit"] if row["id"] == translation["text_unit_id"]
    )
    target.update(lang="en", id="t:en:" + digest(string(target["text"]).encode())[7:23])
    translation.update(
        text_unit_id=target["id"],
        target_lang="en",
        origin="official",
        authority="sve_official",
    )
    counterpart = deepcopy(view["face_revision"][0])
    counterpart.update(
        id="english-revision",
        region="en",
        name_unit_id=target["id"],
        name_concept_id="other-term",
        translations=[],
    )
    view["face_revision"].append(counterpart)
    view["annotation_concept"].append(
        {
            "id": "other-term",
            "category": "card_name",
            "card_ids": [],
            "explanations": [],
        }
    )
    selection = object_value(array(view["face_revision"][0]["translations"])[0])
    selection.update(
        target_lang="en",
        basis="official_counterpart",
        counterpart={
            "owner": {"kind": "face_revision", "id": "english-revision"},
            "field": "name",
            "ordinal": None,
        },
    )
    assert translation["annotation_set_id"] is None
    assert view["field_annotation"] == []
    with pytest.raises(ValueError, match="public-annotation/text_identity"):
        validate_annotations(view, ("ja", "en", "zh-Hant"))
