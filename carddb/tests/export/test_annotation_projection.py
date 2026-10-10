"""Stored semantic positions cross the actual producer and complete wire reader."""

from dataclasses import replace

import pytest

from sve_carddb.build import create_database
from sve_carddb.contracts.annotations import Annotation, AnnotationSet
from sve_carddb.contracts.four_layer import GlossaryReference, Span, hash_payload
from sve_carddb.core.json import digest
from sve_carddb.domains.translations.four_layer_storage import write_annotation
from sve_carddb.export.media import prepare_media
from sve_carddb.export.reader import Fragment, read_snapshot
from sve_carddb.export.semantics import validate_view
from sve_carddb.export.transport import Batch, Ownership, export_snapshot

from ..support.snapshot_project_fixtures import TEXT, populate, schema
from .test_snapshot_project import projected


def annotation(unit: str, *, empty: bool = False) -> AnnotationSet:
    occurrences = (
        ()
        if empty
        else (
            Annotation(
                ordinal=0,
                reference=GlossaryReference(kind="glossary", key="term"),
                ranges=(Span(start=0, end=9),),
                bold=True,
            ),
        )
    )
    payload = {
        "recipe": "annotation-v1",
        "text_unit_id": unit,
        "occurrences": [a.model_dump(mode="json") for a in occurrences],
    }
    return AnnotationSet(
        id="ann:" + hash_payload(payload), text_unit_id=unit, occurrences=occurrences
    )


def test_nonempty_original_and_target_annotations_use_complete_new_wire() -> None:
    with create_database(schema(with_annotations=True)) as db:
        with db.transaction():
            populate(db)
            target_text = "Synthetic translation"
            target_unit = "t:zh-Hant:" + digest(target_text.encode())[7:23]
            db.insert(
                "text_unit",
                {
                    "id": target_unit,
                    "lang": "zh-Hant",
                    "text": target_text,
                    "content_hash": digest(target_text.encode()),
                },
            )
            original, translated = annotation(TEXT), annotation(target_unit)
            write_annotation(db, original)
            write_annotation(db, translated)
            db.insert(
                "translation_use_annotation",
                {"use_id": "use", "annotation_set_id": original.id},
            )
            db.insert(
                "translation_annotation",
                {"translation_id": "translation", "annotation_set_id": translated.id},
            )
        view = projected(db)
        assert view.tables["field_annotation"] == [
            {
                "owner": {"kind": "face_revision", "id": "revision"},
                "field": "name",
                "ordinal": None,
                "annotation_set_id": original.id,
            }
        ]
        assert len(view.tables["annotation_set"]) == 2
        assert view.tables["translation"][0]["annotation_set_id"] == translated.id
        assert view.tables["annotation_concept"] == [
            {
                "id": "term",
                "category": "keyword",
                "explanations": [{"kind": "keyword", "id": "keyword"}],
                "card_ids": [],
            }
        ]
        view = replace(
            view,
            tables=view.tables
            | {
                "printing_image": [
                    row | {"availability": "unfetched", "publication_state": "pending"}
                    for row in view.tables["printing_image"]
                ],
                "image_variant": [],
            },
        )
        view = prepare_media(view, None, revision=1).projection
        snapshot = export_snapshot(
            view,
            Ownership.from_database(db, view),
            Batch("preview-20261010T010203Z-0001", "2026-10-10T01:02:03Z", ("jp",)),
        )
        assert snapshot.manifest["format_version"] == "3.0.0"
        assert (
            read_snapshot(
                snapshot.manifest,
                {key: blob.raw for key, blob in snapshot.payloads.items()},
            )
            == view.tables
        )


def test_empty_private_sets_emit_no_public_id_use_or_fragment() -> None:
    with create_database(schema(with_annotations=True)) as db:
        with db.transaction():
            populate(db)
            empty = annotation(TEXT, empty=True)
            write_annotation(db, empty)
            db.insert(
                "translation_use_annotation",
                {"use_id": "use", "annotation_set_id": empty.id},
            )
        view = projected(db)
        assert view.tables["annotation_set"] == []
        assert view.tables["field_annotation"] == []
        assert view.tables["annotation_concept"] == []
        assert view.tables["translation"][0]["annotation_set_id"] is None


def test_reader_rejects_annotation_uses_outside_source_field_partition() -> None:
    with create_database(schema(with_annotations=True)) as db:
        with db.transaction():
            populate(db)
            original = annotation(TEXT)
            write_annotation(db, original)
            db.insert(
                "translation_use_annotation",
                {"use_id": "use", "annotation_set_id": original.id},
            )
        view = projected(db)
        fragments = [
            Fragment(
                file="",
                table=table,
                value={
                    "owner": {
                        "kind": "home_set" if table == "field_annotation" else "global",
                        "id": "family" if table == "field_annotation" else None,
                    },
                    "partition": "bootstrap",
                    "base": None,
                },
                rows=view.tables[table],
            )
            for table in ("field_annotation", "annotation_set", "annotation_concept")
        ]
        manifest = view.metadata | {"languages": ["ja", "zh-Hant"]}
        validate_view(view.tables, manifest, fragments)
        for fragment in fragments:
            fragment.value["partition"] = "detail"
            with pytest.raises(ValueError, match=r"annotation|Annotation"):
                validate_view(view.tables, manifest, fragments)
            fragment.value["partition"] = "bootstrap"
