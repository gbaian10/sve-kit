"""Historical source corrections retain translations without changing current wording."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build import Json, create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField
from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.text_observations.vocabulary import Vocabulary
from sve_carddb.domains.translations.four_layer_authored import from_files
from sve_carddb.domains.translations.four_layer_build import Settings, apply
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_matching import Frames
from sve_carddb.domains.translations.four_layer_pipeline import (
    compile_card_field,
    prepare_classifier,
)
from sve_carddb.domains.translations.four_layer_sources import descriptor
from sve_carddb.domains.translations.inputs import load_glossary

from ...support.digital_link_import_fixtures import make_fixture
from ...support.translation_fixtures import template
from .test_four_layer_classification import classifier
from .test_four_layer_storage import compiled as compiled  # ruff: ignore[useless-import-alias] -- shared actual SQLite schema

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import CompiledSchema
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding
    from sve_carddb.domains.translations.four_layer_authored import Inputs


def _inputs(frame: Frame, binding: SourceBinding, control: str) -> Inputs:
    records: list[tuple[str, dict[str, JsonValue]]] = [
        (
            "templates/definitions/000.yaml",
            {"kind": "sentence_template", "data": frame.model_dump(mode="json")},
        ),
        (
            "templates/values/000.yaml",
            {
                "kind": "template_translation",
                "origin": "machine",
                "low_confidence": True,
                "data": {
                    "template_id": frame.id,
                    "lang": "zh-Hant",
                    "target": {
                        "format": 1,
                        "nodes": [{"kind": "Literal", "text": "合成規則。"}],
                    },
                },
            },
        ),
    ]
    context: dict[str, JsonValue] = {
        "source_unit_id": binding.source.source_unit_id,
        "variant": "default",
    }
    if control == "manual":
        records.append(
            (
                "overrides/synthetic/000.yaml",
                {
                    "kind": "template_match",
                    "data": {
                        "context_key": context,
                        "source_hash": "sha256:" + binding.source.source_hash,
                        "matches": [
                            {
                                "frame_id": frame.id,
                                "source_span": binding.source_span.model_dump(
                                    mode="json"
                                ),
                                "values": binding.model_dump(mode="json")["values"],
                            }
                        ],
                    },
                },
            )
        )
    if control in {"pin", "suppress"}:
        records.append(
            (
                "overrides/synthetic/001.yaml",
                {
                    "kind": "translation_override",
                    "data": {
                        "context_key": context,
                        "lang": "zh-Hant",
                        "action": control,
                        "templates": [
                            {
                                "template_id": frame.id,
                                "lang": "zh-Hant",
                                "variant_key": "alternative",
                            }
                        ]
                        if control == "pin"
                        else [],
                        "terms": [],
                        "reason": "Synthetic wording selection",
                    },
                },
            )
        )
    if control == "pin":
        records.append(
            (
                "templates/values/001.yaml",
                {
                    "kind": "template_translation_variant",
                    "origin": "machine",
                    "low_confidence": True,
                    "data": {
                        "template_id": frame.id,
                        "lang": "zh-Hant",
                        "variant_key": "alternative",
                        "target": {
                            "format": 1,
                            "nodes": [{"kind": "Literal", "text": "替代合成規則。"}],
                        },
                    },
                },
            )
        )
    files = []
    for path, record in records:
        content = canonical(
            {"format": 3, "kind": "translation_shard", "records": [record]}
        )
        files.append(("translations/" + path, content, content))
    return from_files(tuple(files))


@pytest.mark.parametrize(
    ("difference", "control"),
    [
        (None, "automatic"),
        (None, "manual"),
        (None, "pin"),
        (None, "suppress"),
        ("wording", "automatic"),
        ("face", "automatic"),
        ("cost", "automatic"),
    ],
)
def test_source_correction_reuses_the_same_face_translation(  # ruff: ignore[too-many-locals] -- verify source matching, translated text, exact bindings and current selection together
    tmp_path: Path, compiled: CompiledSchema, difference: str | None, control: str
) -> None:
    fixture = make_fixture(tmp_path, dual=difference == "face")
    root = tmp_path / "authored"
    (root / "translations/glossary").mkdir(parents=True)
    sources = fixture.sources()
    with create_database(compiled) as db, template().copy() as baseline:
        with db.transaction():
            for table in compile_build(("t0", "translation_evidence")).tables:
                for row in baseline.rows(table.name):
                    db.insert(table.name, row.values)
        fixture.publish(db)
        with db.transaction():
            unit = TextInterner(db).intern(
                LocalizedText(lang="ja", text="Synthetic rule.")
            )
            db.update(
                "face_revision", {"id": "link-revision"}, {"effect_unit_id": unit}
            )
            original = db.select(
                "face_revision",
                db.columns("face_revision"),
                where={"id": "link-revision"},
            )[0].values
            if difference == "face":
                db.delete("face_revision", {"id": "other-link-revision-1"})
            db.insert(
                "face_revision",
                dict(original)
                | {
                    "id": "corrected-revision",
                    "revision": 2,
                    "change_kind": "wording"
                    if difference == "wording"
                    else "source_correction",
                    "face_id": fixture.others[0][1].id
                    if difference == "face"
                    else original["face_id"],
                    "cost": 2 if difference == "cost" else original["cost"],
                },
            )
            db.insert(
                "face_current",
                {
                    "face_id": original["face_id"],
                    "region": "jp",
                    "revision_id": "link-revision",
                    "basis": "latest_adopted_wording",
                },
            )
        current = db.rows("face_current")
        engine = prepare_classifier(
            db,
            load_glossary(root),
            sources,
            Vocabulary(bindings=()),
            classifier().rules,
        )
        original_field = OwnerField(
            owner=FaceRevisionOwner(kind="face_revision", revision_id="link-revision"),
            field="effect",
            ordinal=None,
        )
        original_source = descriptor(db, sources, original_field, fixture.jp.batch_id)
        derived = derive_source("Synthetic rule.", original_source, engine)
        frame, binding = derived.bind(0)
        corrected_field = original_field.model_copy(
            update={
                "owner": FaceRevisionOwner(
                    kind="face_revision", revision_id="corrected-revision"
                )
            }
        )
        corrected_source = descriptor(db, sources, corrected_field, fixture.jp.batch_id)
        result = compile_card_field(
            db, sources, corrected_source, engine, Frames((frame,))
        )
        assert result.issues == ("unmatched_source_frame",)
        with db.transaction():
            report = apply(
                db,
                _inputs(frame, binding, control),
                load_glossary(root),
                Settings(
                    sources,
                    Vocabulary(bindings=()),
                    classifier().rules,
                    fixture.jp.batch_id,
                    fixture.authored,
                    "zh-Hant",
                ),
            )
        corrected = next(d for d in report.details if d.field == corrected_field)
        assert corrected.translated is (difference is None and control != "suppress")
        assert corrected.reasons == (
            ("suppressed_translation",)
            if control == "suppress"
            else ()
            if difference is None
            else ("unmatched_source_frame",)
        )
        assert db.rows("face_current") == current
        if difference is not None:
            return
        translations = db.rows("translation")
        assert len(translations) == (0 if control == "suppress" else 2)
        assert all(
            (r.values["text"], r.values["origin"], r.values["low_confidence"])
            == ("替代合成規則。" if control == "pin" else "合成規則。", "machine", True)
            for r in translations
        )
        bindings = db.rows("text_template_binding")
        assert len(bindings) == 2
        assert len({r.values["frame_id"] for r in bindings}) == 2
        stored = db.rows("sentence_template")
        assert {r.values["record_key"] for r in stored} == {
            canonical(["sentence_template", frame.id]).decode()
        }
        assert len({r.values["authored_source_id"] for r in stored}) == 1
        for row in bindings:
            source = row.values["source"]
            assert isinstance(source, Json)
            semantic = next(
                r.values["semantic_variant"]
                for r in stored
                if r.values["id"] == row.values["frame_id"]
            )
            assert isinstance(semantic, Json)
            assert object_value(semantic.value)["state"] == "pending"
            scope = object_value(object_value(semantic.value)["scope"])
            assert scope["owner"] == object_value(source.value)["owner"]
        assert {r.values["record_key"] for r in db.rows("template_translation")} == {
            canonical(["template_translation", frame.id, "zh-Hant"]).decode(),
            *(
                [
                    canonical(
                        [
                            "template_translation_variant",
                            frame.id,
                            "zh-Hant",
                            "alternative",
                        ]
                    ).decode()
                ]
                if control == "pin"
                else []
            ),
        }
