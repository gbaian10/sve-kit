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
from sve_carddb.domains.translations.four_layer_authored import (
    FrameRecord,
    TargetRecord,
    TargetVariantRecord,
    from_files,
)
from sve_carddb.domains.translations.four_layer_build import Settings, apply
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_matching import Frames
from sve_carddb.domains.translations.four_layer_pipeline import (
    compile_card_field,
    prepare_classifier,
)
from sve_carddb.domains.translations.four_layer_sources import descriptor
from sve_carddb.domains.translations.inputs import load_glossary

from ...support import digital_link_import_fixtures
from ...support.digital_link_import_fixtures import make_fixture
from ...support.translation_fixtures import template
from .test_four_layer_classification import classifier
from .test_four_layer_storage import compiled as compiled  # ruff: ignore[useless-import-alias] -- shared actual SQLite schema

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.build import CompiledSchema, Database, Value
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding
    from sve_carddb.domains.translations.four_layer_authored import Inputs

    from ...support.digital_link_import_fixtures import Fixture


def _inputs(
    frame: Frame, binding: SourceBinding, control: str, text: str = "合成規則。"
) -> Inputs:
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
                        "nodes": [
                            {"kind": "LeafRef", "slot": slot.name}
                            for slot in frame.leaf_schema.slots
                        ]
                        if frame.role == "layout"
                        else [{"kind": "Literal", "text": text}],
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


def _publish(
    db: Database, fixture: Fixture, text: str = "Synthetic rule."
) -> Mapping[str, Value]:
    with template().copy() as baseline, db.transaction():
        for table in compile_build(("t0", "translation_evidence")).tables:
            for row in baseline.rows(table.name):
                db.insert(table.name, row.values)
    fixture.publish(db)
    with db.transaction():
        unit = TextInterner(db).intern(LocalizedText(lang="ja", text=text))
        db.update("face_revision", {"id": "link-revision"}, {"effect_unit_id": unit})
        db.insert(
            "face_current",
            {
                "face_id": fixture.face.id,
                "region": "jp",
                "revision_id": "link-revision",
                "basis": "latest_adopted_wording",
            },
        )
    return db.select(
        "face_revision", db.columns("face_revision"), where={"id": "link-revision"}
    )[0].values


def _combine(*inputs: Inputs, excluded_frames: tuple[str, ...] = ()) -> Inputs:
    selected = {
        r.record_key: r
        for snapshot in inputs
        for r in snapshot.records
        if (
            r.data.id
            if isinstance(r, FrameRecord)
            else r.data.template_id
            if isinstance(r, (TargetRecord, TargetVariantRecord))
            else None
        )
        not in excluded_frames
    }
    files = []
    for index, record in enumerate(selected.values()):
        area = "definitions" if isinstance(record, FrameRecord) else "values"
        content = canonical(
            {
                "format": 3,
                "kind": "translation_shard",
                "records": [record.model_dump(mode="json", exclude={"record_key"})],
            }
        )
        files.append(
            (f"translations/templates/{area}/{index:03}.yaml", content, content)
        )
    return from_files(tuple(files))


def _field_inputs(
    db: Database,
    fixture: Fixture,
    field: OwnerField,
    raw: str,
    translated: str = "合成規則。",
) -> Inputs:
    sources = fixture.sources()
    engine = prepare_classifier(
        db,
        load_glossary(fixture.root / "authored"),
        sources,
        Vocabulary(bindings=()),
        classifier().rules,
    )
    source = descriptor(db, sources, field, fixture.jp.batch_id)
    derived = derive_source(raw, source, engine)
    return _combine(
        *(
            _inputs(*derived.bind(part.ordinal), "automatic", translated)
            for part in derived.field.parts
        )
    )


def _apply(db: Database, fixture: Fixture, inputs: Inputs) -> None:
    with db.transaction():
        apply(
            db,
            inputs,
            load_glossary(fixture.root / "authored"),
            Settings(
                fixture.sources(),
                Vocabulary(bindings=()),
                classifier().rules,
                fixture.jp.batch_id,
                fixture.authored,
                "zh-Hant",
            ),
        )


def _translated(db: Database, field: OwnerField) -> str:
    assert isinstance(field.owner, FaceRevisionOwner)
    use = db.select(
        "translation_use",
        ("context_id",),
        where={
            "face_revision_id": field.owner.revision_id,
            "field": field.field,
            "ordinal": field.ordinal,
        },
    )[0]
    selected = db.select(
        "translation_selection",
        ("translation_id",),
        where={"context_id": use.values["context_id"], "target_lang": "zh-Hant"},
    )[0]
    text = db.select(
        "translation", ("text",), where={"id": selected.values["translation_id"]}
    )[0].values["text"]
    assert isinstance(text, str)
    return text


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
        ("trait", "automatic"),
        ("title", "automatic"),
        ("special_kind", "automatic"),
    ],
)
def test_source_correction_reuses_the_same_face_translation(  # ruff: ignore[too-many-locals] -- verify source matching, translated text, exact bindings and current selection together
    tmp_path: Path, compiled: CompiledSchema, difference: str | None, control: str
) -> None:
    fixture = make_fixture(tmp_path, dual=difference == "face")
    root = tmp_path / "authored"
    (root / "translations/glossary").mkdir(parents=True)
    sources = fixture.sources()
    with create_database(compiled) as db:
        original = _publish(db, fixture)
        with db.transaction():
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
            if difference in {"trait", "title", "special_kind"}:
                db.insert(
                    "vocabulary",
                    {
                        "kind": difference,
                        "code": "synthetic-difference",
                        "label_unit_id": original["name_unit_id"],
                        "active": True,
                    },
                )
                db.insert(
                    "face_" + difference,
                    {
                        "revision_id": "corrected-revision",
                        difference + "_code": "synthetic-difference",
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
        assert compile_card_field(
            db, sources, corrected_source, engine, Frames((frame,))
        ).issues == ("unmatched_source_frame",)
        authored = _inputs(frame, binding, control)
        with db.transaction():
            report = apply(
                db,
                authored,
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
            record.record_key
            for record in authored.records
            if isinstance(record, FrameRecord)
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
            record.record_key
            for record in authored.records
            if isinstance(record, (TargetRecord, TargetVariantRecord))
        }


@pytest.mark.parametrize(
    "section", [True, False], ids=["supplemental-section", "resolved-and-pending"]
)
def test_correction_inherits_sections_and_keeps_resolved_frames(
    tmp_path: Path,
    compiled: CompiledSchema,
    monkeypatch: pytest.MonkeyPatch,
    section: bool,
) -> None:
    raw = "Synthetic section." if section else "Synthetic first.\nSynthetic second."
    html = (
        "Synthetic rule.<br>-----<br>" + raw if section else raw.replace("\n", "<br>")
    )
    monkeypatch.setattr(
        digital_link_import_fixtures,
        "RAW",
        digital_link_import_fixtures.RAW.replace(b"Synthetic rule.", html.encode()),
    )
    fixture = make_fixture(tmp_path)
    (fixture.root / "authored/translations/glossary").mkdir(parents=True)
    with create_database(compiled) as db:
        original = _publish(db, fixture, "Synthetic rule." if section else raw)
        with db.transaction():
            db.insert(
                "face_revision",
                dict(original)
                | {
                    "id": "corrected-revision",
                    "revision": 2,
                    "change_kind": "source_correction",
                },
            )
            if section:
                unit = TextInterner(db).intern(LocalizedText(lang="ja", text=raw))
                for revision in ("link-revision", "corrected-revision"):
                    db.insert(
                        "face_text_section",
                        {
                            "revision_id": revision,
                            "ordinal": 0,
                            "kind": "unknown",
                            "text_unit_id": unit,
                        },
                    )
        current = db.rows("face_current")
        field = OwnerField(
            owner=FaceRevisionOwner(kind="face_revision", revision_id="link-revision"),
            field="section" if section else "effect",
            ordinal=0 if section else None,
        )
        authored = _field_inputs(db, fixture, field, raw)
        if section:
            authored = _combine(
                authored,
                _field_inputs(
                    db,
                    fixture,
                    field.model_copy(update={"field": "effect", "ordinal": None}),
                    "Synthetic rule.",
                ),
            )
        _apply(db, fixture, authored)
        corrected = field.model_copy(
            update={
                "owner": FaceRevisionOwner(
                    kind="face_revision", revision_id="corrected-revision"
                )
            }
        )
        assert _translated(db, corrected) == (
            "合成規則。" if section else "合成規則。\n合成規則。"
        )
        assert _translated(db, field) == _translated(db, corrected)
        assert db.rows("face_current") == current
        frames = [r.data for r in authored.records if isinstance(r, FrameRecord)]
        resolved = {
            frame.id for frame in frames if frame.semantic_variant.state == "resolved"
        }
        pending = {
            frame.id for frame in frames if frame.semantic_variant.state == "pending"
        }
        if not section:
            assert resolved
            assert pending
        use = db.select(
            "translation_use",
            ("id",),
            where={
                "face_revision_id": "corrected-revision",
                "field": field.field,
                "ordinal": field.ordinal,
            },
        )[0]
        bound = {
            r.values["frame_id"]
            for r in db.select(
                "text_template_binding",
                ("frame_id",),
                where={"use_id": use.values["id"]},
            )
        }
        assert resolved <= bound
        assert pending.isdisjoint(bound)
        assert {r.values["record_key"] for r in db.rows("sentence_template")} == {
            r.record_key for r in authored.records if isinstance(r, FrameRecord)
        }


@pytest.mark.parametrize(
    "nearest_complete",
    [True, False],
    ids=["nearest-complete", "skip-incomplete-nearest"],
)
def test_correction_chooses_the_nearest_completely_matched_revision(
    tmp_path: Path,
    compiled: CompiledSchema,
    monkeypatch: pytest.MonkeyPatch,
    nearest_complete: bool,
) -> None:
    raw = "Synthetic first.\nSynthetic second."
    monkeypatch.setattr(
        digital_link_import_fixtures,
        "RAW",
        digital_link_import_fixtures.RAW.replace(
            b"Synthetic rule.", raw.replace("\n", "<br>").encode()
        ),
    )
    fixture = make_fixture(tmp_path)
    (fixture.root / "authored/translations/glossary").mkdir(parents=True)
    with create_database(compiled) as db:
        original = _publish(db, fixture, raw)
        with db.transaction():
            db.insert(
                "face_revision",
                dict(original)
                | {"id": "nearest-revision", "revision": 2, "change_kind": "wording"},
            )
            db.insert(
                "face_revision",
                dict(original)
                | {
                    "id": "corrected-revision",
                    "revision": 3,
                    "change_kind": "source_correction",
                },
            )
        current = db.rows("face_current")
        field = OwnerField(
            owner=FaceRevisionOwner(kind="face_revision", revision_id="link-revision"),
            field="effect",
            ordinal=None,
        )
        earlier = _field_inputs(db, fixture, field, raw, "較早合成譯文。")
        nearest_field = field.model_copy(
            update={
                "owner": FaceRevisionOwner(
                    kind="face_revision", revision_id="nearest-revision"
                )
            }
        )
        nearest = _field_inputs(db, fixture, nearest_field, raw, "較近合成譯文。")
        excluded = (
            ()
            if nearest_complete
            else (
                max(
                    (
                        r.data
                        for r in nearest.records
                        if isinstance(r, FrameRecord)
                        and r.data.semantic_variant.scope is not None
                    ),
                    key=lambda frame: (
                        frame.semantic_variant.scope.line_ordinal
                        if frame.semantic_variant.scope
                        else -1
                    ),
                ).id,
            )
        )
        _apply(db, fixture, _combine(earlier, nearest, excluded_frames=excluded))
        corrected = field.model_copy(
            update={
                "owner": FaceRevisionOwner(
                    kind="face_revision", revision_id="corrected-revision"
                )
            }
        )
        expected = "較近合成譯文。" if nearest_complete else "較早合成譯文。"
        assert _translated(db, corrected) == expected + "\n" + expected
        assert _translated(db, field) == "較早合成譯文。\n較早合成譯文。"
        assert db.rows("face_current") == current
