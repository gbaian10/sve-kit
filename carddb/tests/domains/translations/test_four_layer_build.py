"""The producer projects only complete fields proved against a real sealed batch."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField, Target
from sve_carddb.core.json import canonical
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.text_observations.vocabulary import Vocabulary
from sve_carddb.domains.translations.four_layer_authored import (
    FrameRecord,
    TargetRecord,
    from_files,
)
from sve_carddb.domains.translations.four_layer_build import Settings, apply
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.four_layer_pipeline import prepare_classifier
from sve_carddb.domains.translations.four_layer_sources import descriptor
from sve_carddb.domains.translations.four_layer_storage import read_binding
from sve_carddb.domains.translations.inputs import load_glossary

from ...support.digital_link_import_fixtures import make_fixture
from ...support.translation_fixtures import template
from .test_four_layer_classification import classifier
from .test_four_layer_storage import compiled as compiled  # ruff: ignore[useless-import-alias] -- shared actual SQLite schema

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import CompiledSchema


@pytest.mark.parametrize("translated", [True, False])
def test_producer_uses_exact_owner_and_preserves_whole_field_fallback(  # ruff: ignore[too-many-locals] -- independent frozen source, authored inputs and actual DB results form the end-to-end assertion
    tmp_path: Path, compiled: CompiledSchema, translated: bool
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
        engine = prepare_classifier(db, snapshot, sources, vocabulary, rules)
        normalized = normalize_source("Synthetic rule.", source)
        part = normalized.parts[0]
        frame, _ = engine.recognize(
            "Synthetic rule.", source, part, field=normalized
        ).bind(source, part)
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
        assert report.fields == 1
        assert report.translated == int(translated)
        bindings = db.rows("text_template_binding")
        assert len(bindings) == 1
        assert (
            read_binding(db, str(bindings[0].values["id"]), engine.domains).source
            == source
        )
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
