"""The producer projects only complete fields proved against a real sealed batch."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json, create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField, Target
from sve_carddb.contracts.rulings import LegacyReference
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.effects.classifications import Basis
from sve_carddb.domains.effects.reader import Catalog
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.rulings.historical import Observations, validate
from sve_carddb.domains.rulings.historical import rebuild as rebuild_rulings
from sve_carddb.domains.rulings.storage import write as write_rulings
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.text_observations.vocabulary import Vocabulary
from sve_carddb.domains.translations.four_layer_authored import (
    FormRecord,
    FrameRecord,
    TargetRecord,
    TargetVariantRecord,
    from_files,
)
from sve_carddb.domains.translations.four_layer_build import Settings, apply
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.four_layer_pipeline import prepare_classifier
from sve_carddb.domains.translations.four_layer_sources import descriptor
from sve_carddb.domains.translations.inputs import load_glossary

from ...support import digital_link_import_fixtures
from ...support.build_db_fixtures import rows
from ...support.digital_link_import_fixtures import make_fixture
from ...support.translation_fixtures import template
from ..effects.test_classifications import classification, member
from ..rulings.test_rulings import ruling
from .test_four_layer_classification import classifier
from .test_four_layer_storage import compiled as compiled  # ruff: ignore[useless-import-alias] -- shared actual SQLite schema

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import CompiledSchema


@pytest.mark.parametrize("stride", [None, "2"])
def test_profile_stride_limits_fields_only_when_configured(
    tmp_path: Path,
    compiled: CompiledSchema,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    stride: str | None,
) -> None:
    monkeypatch.delenv("SVE_PROFILE_FIELD_STRIDE", raising=False)
    if stride is not None:
        monkeypatch.setenv("SVE_PROFILE_FIELD_STRIDE", stride)
    fixture = make_fixture(tmp_path, dual=True)
    (fixture.root / "authored/translations/glossary").mkdir(parents=True)
    snapshot = load_glossary(fixture.root / "authored")
    settings = Settings(
        fixture.sources(),
        Vocabulary(bindings=()),
        classifier().rules,
        fixture.jp.batch_id,
        fixture.authored,
        "zh-Hant",
    )
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
            for revision in ("link-revision", "other-link-revision-1"):
                db.update("face_revision", {"id": revision}, {"effect_unit_id": unit})
            report = apply(db, from_files(()), snapshot, settings)
        expected = {"link-revision"}
        if stride is None:
            expected.add("other-link-revision-1")
        assert report.fields == len(expected)
        assert {
            detail.field.owner.revision_id
            for detail in report.details
            if isinstance(detail.field.owner, FaceRevisionOwner)
        } == expected
        assert report.reasons == {"unmatched_source_frame": len(expected)}
    assert ("sampling mode" in capsys.readouterr().err) is (stride is not None)


@pytest.mark.parametrize("translated", [True, False])
@pytest.mark.parametrize("unused", [True, False])
def test_producer_uses_exact_owner_and_preserves_whole_field_fallback(  # ruff: ignore[too-many-locals,too-many-statements] -- independent frozen source, authored inputs and actual DB results form the end-to-end assertion
    tmp_path: Path, compiled: CompiledSchema, translated: bool, unused: bool
) -> None:
    fixture = make_fixture(tmp_path)
    root = tmp_path / "authored"
    (root / "translations/glossary").mkdir(parents=True)
    snapshot = load_glossary(root)
    sources = fixture.sources()
    vocabulary = Vocabulary(bindings=())
    rules = classifier().rules
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
        field = OwnerField(
            owner=FaceRevisionOwner(kind="face_revision", revision_id="link-revision"),
            field="effect",
            ordinal=None,
        )
        source = descriptor(db, sources, field, fixture.jp.batch_id)
        with db.transaction():
            db.insert(
                "printing",
                rows()["printing"]
                | {
                    "id": "printing:sample",
                    "card_id": fixture.card.id,
                    "card_no": fixture.printing.card_no,
                    "home_set_id": fixture.printing.home_set_id,
                    "source_id": source.source_ref.source_version_id,
                },
            )
            db.insert(
                "printing_face",
                rows()["printing_face"]
                | {
                    "printing_id": "printing:sample",
                    "face_id": fixture.face.id,
                    "card_id": fixture.card.id,
                    "printed_name_unit_id": None,
                    "printed_effect_unit_id": None,
                    "printed_text_state": "unknown",
                    "source_id": source.source_ref.source_version_id,
                },
            )
        engine = prepare_classifier(db, snapshot, sources, vocabulary, rules)
        authored_text = "Unused synthetic rule." if unused else "Synthetic rule."
        authored_source = source.model_copy(
            update={
                "source_hash": digest(authored_text.encode())[7:],
                "source_ref": source.source_ref.model_copy(
                    update={"text_hash": digest(authored_text.encode())[7:]}
                ),
            }
        )
        normalized = normalize_source(authored_text, authored_source)
        part = normalized.parts[0]
        frame, _ = engine.recognize(
            authored_text, authored_source, part, field=normalized
        ).bind(authored_source, part)
        definition = FrameRecord(kind="sentence_template", data=frame)
        records = [definition.model_dump(mode="json", exclude={"record_key"})]
        if translated:
            target = TargetRecord.model_validate_json(
                canonical(
                    {
                        "kind": "template_translation",
                        "origin": "machine",
                        "low_confidence": True,
                        "data": {
                            "template_id": frame.id,
                            "lang": "zh-Hant",
                            "target": Target.model_validate_json(
                                canonical(
                                    {
                                        "format": 1,
                                        "nodes": [
                                            {"kind": "Literal", "text": "合成規則。"}
                                        ],
                                    }
                                )
                            ).model_dump(mode="json"),
                        },
                    }
                )
            )
            records.append(target.model_dump(mode="json", exclude={"record_key"}))
        files = []
        for record in records:
            area = "definitions" if record["kind"] == "sentence_template" else "values"
            raw = canonical(
                {"format": 3, "kind": "translation_shard", "records": [record]}
            )
            files.append((f"translations/templates/{area}/000.yaml", raw, raw))
        inputs = from_files(tuple(files))
        settings = Settings(
            sources, vocabulary, rules, fixture.jp.batch_id, fixture.authored, "zh-Hant"
        )
        with db.transaction():
            report = apply(db, inputs, snapshot, settings)
        actual_part = normalize_source("Synthetic rule.", source).parts[0]
        historical = LegacyReference.model_validate_json(
            canonical(
                {
                    "legacy": "T0123456789",
                    "namespace": "classification-v0",
                    "uses": [
                        {
                            "cards": [fixture.printing.card_no],
                            "occurrence": {
                                **source.model_dump(
                                    mode="json",
                                    include={
                                        "owner",
                                        "field",
                                        "ordinal",
                                        "source_hash",
                                    },
                                ),
                                "line_ordinal": 0,
                                "role": "body",
                                "segments": [
                                    s.model_dump(mode="json")
                                    for s in actual_part.source_span.segments
                                ],
                            },
                        }
                    ],
                }
            )
        )
        documents = (ruling(applies_to=[historical.model_dump(mode="json")]),)
        old_member = member().model_copy(
            update={
                "body_hash": digest(b"Synthetic rule."),
                "body_length": len("Synthetic rule."),
                "position": member().position.model_copy(
                    update={"card_no": fixture.printing.card_no}
                ),
            }
        )
        catalog = Catalog(
            "classification-v0",
            Basis(recipe="fixture-v1", inputs_hash=digest(b"inputs")),
            (
                classification().model_copy(
                    update={"id": historical.legacy, "members": (old_member.id,)}
                ),
            ),
            (old_member,),
        )
        validate(documents, catalog, Observations(db))
        with pytest.raises(ValueError, match="absent from the catalog"):
            validate(documents, replace(catalog, classifications=()), Observations(db))

        with db.transaction():
            ruling_report = write_rulings(
                db,
                documents,
                fixture.authored,
                rebuild=rebuild_rulings(documents, report.compiled),
            )
        assert ruling_report.resolutions[0].reason == (
            "no_candidate" if unused else "unsupported_relation"
        )
        assert ruling_report.payload()["active_template_edges"] == 0
        assert len(ruling_report.resolutions[0].candidates) == (0 if unused else 1)
        assert report.fields == 1
        detail = report.details[0].payload()
        assert detail["owner"] == {
            "kind": "face_revision",
            "revision_id": "link-revision",
        }
        assert detail["field"] == "effect"
        assert detail["translated"] == (translated and not unused)
        assert detail["source_hash"] == source.source_hash
        assert report.payload()["np_target_bindings"] == 0
        if unused:
            assert detail["reasons"] == ["unmatched_source_frame"]
            assert detail["projections"] == {}
            assert detail["target_bindings"] == 0
        else:
            assert detail["pending_semantics"] == [frame.id]
            assert detail["projections"] == {"pending": 1}
            assert detail["target_bindings"] == int(translated)
            assert detail["low_confidence"] == (True if translated else None)
            assert detail["reasons"] == (
                [] if translated else ["missing_template_translation"]
            )
        assert "Synthetic rule." not in canonical(report.payload()).decode()
        assert report.translated == int(translated and not unused)
        assert report.unused_frames == ((frame.id,) if unused else ())
        if unused:
            assert report.payload()["unused_authored_frames"] == [
                {"frame_id": frame.id, "reason": "no_fully_verified_source_owner_field"}
            ]
            assert db.rows("sentence_template") == ()
            assert db.rows("template_translation") == ()
            assert db.rows("translation_selection") == ()
            assert report.reasons == {"unmatched_source_frame": 1}
            return
        bindings = db.rows("text_template_binding")
        assert len(bindings) == 1
        assert bindings[0].values["source"] == Json(source.model_dump(mode="json"))
        if translated:
            value = db.rows("translation")[0].values
            assert (value["text"], value["origin"], value["low_confidence"]) == (
                "合成規則。",
                "machine",
                True,
            )
        else:
            assert db.rows("translation_selection") == ()
            assert report.reasons == {"missing_template_translation": 1}


def test_producer_stores_language_forms_before_targets_that_use_them(  # ruff: ignore[too-many-locals] -- a sealed source, authored form and target jointly reproduce the real integration ordering failure
    tmp_path: Path, compiled: CompiledSchema, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw = "これを2回まで行う。"
    assert b"Synthetic rule." in digital_link_import_fixtures.RAW
    monkeypatch.setattr(
        digital_link_import_fixtures,
        "RAW",
        digital_link_import_fixtures.RAW.replace(b"Synthetic rule.", raw.encode()),
    )
    fixture = make_fixture(tmp_path)
    root = tmp_path / "authored"
    (root / "translations/glossary").mkdir(parents=True)
    snapshot = load_glossary(root)
    sources = fixture.sources()
    vocabulary = Vocabulary(bindings=())
    rules = classifier().rules
    with create_database(compiled) as db, template().copy() as baseline:
        with db.transaction():
            for table in compile_build(("t0", "translation_evidence")).tables:
                for row in baseline.rows(table.name):
                    db.insert(table.name, row.values)
        fixture.publish(db)
        with db.transaction():
            unit = TextInterner(db).intern(LocalizedText(lang="ja", text=raw))
            db.update(
                "face_revision", {"id": "link-revision"}, {"effect_unit_id": unit}
            )
        owner = OwnerField(
            owner=FaceRevisionOwner(kind="face_revision", revision_id="link-revision"),
            field="effect",
            ordinal=None,
        )
        source = descriptor(db, sources, owner, fixture.jp.batch_id)
        engine = prepare_classifier(db, snapshot, sources, vocabulary, rules)
        field = normalize_source(raw, source)
        frame, _ = engine.recognize(raw, source, field.parts[0], field=field).bind(
            source, field.parts[0]
        )
        form = FormRecord.model_validate_json(
            canonical(
                {
                    "kind": "translation_form",
                    "data": {
                        "id": "quantity.repeat_count.value",
                        "lang": "zh-Hant",
                        "rule": "quantity.constant_value.v1",
                        "signature": [
                            {
                                "name": "quantity",
                                "type": "QuantitySpec",
                                "role": "repeat_count",
                            }
                        ],
                        "cases": {
                            "default": [{"kind": "QuantityValue", "arg": "quantity"}]
                        },
                    },
                }
            )
        )
        target = TargetRecord.model_validate_json(
            canonical(
                {
                    "kind": "template_translation",
                    "data": {
                        "template_id": frame.id,
                        "lang": "zh-Hant",
                        "target": {
                            "format": 1,
                            "nodes": [
                                {"kind": "Literal", "text": "最多重複"},
                                {
                                    "kind": "Form",
                                    "form_id": form.data.id,
                                    "args": {"quantity": {"slot": "leaf_0"}},
                                },
                                {"kind": "Literal", "text": "次。"},
                            ],
                        },
                    },
                }
            )
        )
        variant = TargetVariantRecord.model_validate_json(
            canonical(
                {
                    "kind": "template_translation_variant",
                    "data": target.data.model_dump(mode="json")
                    | {"variant_key": "alternative"},
                }
            )
        )
        files = []
        for path, record in (
            (
                "translations/templates/definitions/000.yaml",
                FrameRecord(kind="sentence_template", data=frame),
            ),
            ("translations/templates/values/000.yaml", target),
            ("translations/templates/values/001.yaml", variant),
            ("translations/forms/000.yaml", form),
        ):
            content = canonical(
                {
                    "format": 3,
                    "kind": "translation_shard",
                    "records": [record.model_dump(mode="json", exclude={"record_key"})],
                }
            )
            files.append((path, content, content))
        inputs = from_files(tuple(files))
        with db.transaction():
            report = apply(
                db,
                inputs,
                snapshot,
                Settings(
                    sources,
                    vocabulary,
                    rules,
                    fixture.jp.batch_id,
                    fixture.authored,
                    "zh-Hant",
                ),
            )
        assert report.translated == 1
        assert db.rows("translation")[0].values["text"] == "最多重複2次。"
        targets = db.rows("template_translation")
        assert {row.values["variant_key"] for row in targets} == {
            "default",
            "alternative",
        }
        assert all(
            row.values["target"] == Json(target.data.target.model_dump(mode="json"))
            for row in targets
        )
