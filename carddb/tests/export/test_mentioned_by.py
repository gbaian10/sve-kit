"""Card-name back references derive from current text, never official related rows."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.core.json import array, canonical, object_value, parse
from sve_carddb.domains.translations.names.annotations import annotate_name
from sve_carddb.export.media import prepare_media
from sve_carddb.export.project.mentioned_by import mentioned_by
from sve_carddb.export.reader import Fragment, read_snapshot
from sve_carddb.export.semantics import validate_placement
from sve_carddb.export.transport import Ownership, export_snapshot

from ..support.snapshot_contract_fixtures import _reseal, fixture
from ..support.snapshot_media_fixtures import media_rows
from ..support.snapshot_project_fixtures import (
    TEXT,
    decisions,
    extras,
    populate,
    schema,
)
from .test_snapshot_export import BATCH
from .test_snapshot_project import projected

if TYPE_CHECKING:
    from sve_carddb.export.project.source import Record, View


@pytest.mark.parametrize("field", ["effect", "section", "name", "historical"])
def test_build_and_wire_invert_only_current_effects_and_sections(field: str) -> None:
    with create_database(schema(with_annotations=True)) as db:
        with db.transaction():
            populate(db)
            db.update("glossary_term", {"id": "term"}, {"category": "card_name"})
            family = db.select("product_family", db.columns("product_family"))[0]
            db.insert(
                "product_family",
                dict(family.values)
                | {"id": "other-family", "code": "other", "public_code": "OTHER"},
            )
            db.update("card", {"id": "old_card"}, {"home_set_id": "other-family"})
            revision = "revision"
            if field == "historical":
                row = db.select("face_revision", db.columns("face_revision"))[0]
                db.insert(
                    "face_revision", dict(row.values) | {"id": "history", "revision": 1}
                )
                revision = "history"
            use = extras()["translation_use"] | {
                "id": "effect-use",
                "field": "effect" if field == "historical" else field,
                "ordinal": 0 if field == "section" else None,
                "face_revision_id": revision,
            }
            if field == "name":
                annotate_name(db, "use", TEXT, "term")
            else:
                db.insert("translation_use", use)
                annotate_name(db, "effect-use", TEXT, "term")
            if field == "effect":
                db.insert(
                    "translation_use",
                    use | {"id": "section-use", "field": "section", "ordinal": 0},
                )
                annotate_name(db, "section-use", TEXT, "term")
        chosen = replace(
            decisions(),
            card_name_targets={
                "term": ("old_card", "card", "old_card", "missing-card")
            },
        )
        projection = projected(db, chosen)
        expected = (
            [{"card_id": "old_card", "region": "jp", "from_card_id": "card"}]
            if field in {"effect", "section"}
            else []
        )
        assert projection.tables["card_mentioned_by"] == expected
        projection = replace(projection, tables=projection.tables | media_rows())
        projection = prepare_media(projection, None, revision=1).projection
        snapshot = export_snapshot(
            projection, Ownership.from_database(db, projection), BATCH
        )
        assert (
            read_snapshot(
                snapshot.manifest,
                {key: blob.raw for key, blob in snapshot.payloads.items()},
            )["card_mentioned_by"]
            == expected
        )
        baseline = export_snapshot(
            replace(projection, tables=projection.tables | {"card_mentioned_by": []}),
            Ownership.from_database(db, projection),
            BATCH,
        )
        assert snapshot.payloads.keys() == baseline.payloads.keys()
        files = [object_value(raw) for raw in array(snapshot.manifest["files"])]
        for file in files:
            if any(
                array(count)[0] == "card_mentioned_by"
                for count in array(file["row_counts"])
            ):
                assert file["key"] == "engine/shared/family-other-family/0"
                payload = object_value(parse(snapshot.payloads[str(file["key"])].raw))
                fragment = array(
                    array(object_value(payload["tables"])["card_mentioned_by"])[0]
                )
                assert fragment[0] == ["home_set", "other-family"]


def test_english_uses_selected_japanese_sources_and_all_source_sections() -> None:
    owner: Record = {"kind": "face_revision", "id": "jp-history-donor"}
    pointer: Record = {"owner": owner, "field": "effect", "ordinal": None}
    view: View = {
        "card": [
            {"id": value}
            for value in (
                "source",
                "jp-target",
                "section-target",
                "translated-target",
                "unselected-target",
            )
        ],
        "face": [
            {"card_id": "source", "current": [{"region": "en", "revision_id": "en"}]}
        ],
        "face_revision": [
            {
                "id": "en",
                "translations": [
                    {
                        "field": "effect",
                        "source": pointer,
                        "basis": "jp_source",
                        "translation_id": "zh",
                    }
                ],
            }
        ],
        "translation": [{"id": "zh", "annotation_set_id": "translated"}],
    }
    originals: list[Record] = [
        pointer | {"annotation_set_id": "body"},
        pointer | {"field": "section", "ordinal": 9, "annotation_set_id": "section"},
        pointer
        | {
            "owner": {"kind": "face_revision", "id": "unselected"},
            "annotation_set_id": "unselected",
        },
    ]
    annotations: dict[str, Record] = {
        key: {"occurrences": [{"reference": {"kind": "card_name", "term_id": key}}]}
        for key in ("body", "section", "translated", "unselected")
    }
    targets = {
        "body": ("jp-target",),
        "section": ("section-target",),
        "translated": ("translated-target",),
        "unselected": ("unselected-target",),
    }
    assert mentioned_by(view, originals, annotations, targets) == [
        {"card_id": value, "region": "en", "from_card_id": "source"}
        for value in ("jp-target", "section-target", "translated-target")
    ]


@pytest.mark.parametrize("mutation", ["regional_file", "undeclared_region", "self"])
def test_reader_rejects_invalid_shared_back_references(mutation: str) -> None:
    native = object_value(fixture("mentioned-by-native.json"))
    manifest = object_value(native["manifest"])
    payloads = object_value(native["payloads"])
    key = "engine/shared/family-family/0"
    file = next(
        object_value(f)
        for f in array(manifest["files"])
        if object_value(f)["key"] == key
    )
    packed = array(
        array(object_value(object_value(payloads[key])["tables"])["card_mentioned_by"])[
            0
        ]
    )
    row = array(array(packed[4])[0])
    if mutation == "regional_file":
        view = read_snapshot(
            manifest, {key: canonical(value) for key, value in payloads.items()}
        )
        file["region"] = "jp"
        fragment = Fragment(
            key,
            "card_mentioned_by",
            {
                "owner": {"kind": "home_set", "id": "family"},
                "partition": "detail",
                "segment": "family-family:band-0",
                "base": None,
            },
            view["card_mentioned_by"],
        )
        with pytest.raises(ValueError, match="back references require shared engine"):
            validate_placement(view, manifest, [fragment])
        return
    if mutation == "undeclared_region":
        row[1] = "en"
    else:
        row[2] = row[0]
    with pytest.raises(ValueError, match="back reference"):
        read_snapshot(manifest, _reseal(manifest, payloads))
