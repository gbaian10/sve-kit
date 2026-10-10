"""Whole fields persist typed bindings, exact selections and owner-specific output paths."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import CardNameReference, Target
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.translations.four_layer_authored import (
    FrameRecord,
    TargetRecord,
)
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_fields import render_field
from sve_carddb.domains.translations.four_layer_matching import Matched
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.four_layer_pipeline import CompiledField
from sve_carddb.domains.translations.four_layer_render import (
    Label,
    Renderer,
    SelectedTarget,
)
from sve_carddb.domains.translations.four_layer_sources import CardSource
from sve_carddb.domains.translations.four_layer_storage import (
    read_annotation,
    read_binding,
    read_owned_render_occurrences,
    write_frame,
    write_target,
)

from .test_four_layer_classification import classifier
from .test_four_layer_storage import compiled as compiled  # ruff: ignore[useless-import-alias] -- shared actual SQLite fixture
from .test_four_layer_storage import default_target
from .test_four_layer_storage import stored as stored  # ruff: ignore[useless-import-alias] -- shared exact source fixture

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding


def compiled_field(db: Database, frame: Frame, binding: SourceBinding) -> CompiledField:
    text = db.select(
        "text_unit", ("text",), where={"id": binding.source.source_unit_id}
    )[0].values["text"]
    assert isinstance(text, str)
    return CompiledField(
        CardSource(binding.source, text, "card", "face", "source"),
        normalize_source(text, binding.source),
        (Matched(frame, binding),),
        (False,),
        (),
    )


@pytest.mark.parametrize("translated", [True, False])
def test_whole_field_writes_selection_or_keeps_an_exact_source_fallback(
    stored: tuple[Database, Frame, SourceBinding], translated: bool
) -> None:
    db, frame, binding = stored
    targets = (
        {
            (frame.id, "zh-Hant"): SelectedTarget(
                default_target(),
                origin="machine",
                low_confidence=True,
            )
        }
        if translated
        else {}
    )
    with db.transaction():
        db.delete("translation_use", {"id": "use:synthetic"})
        result = render_field(
            db,
            compiled_field(db, frame, binding),
            Renderer({}, {}, {}),
            targets,
            "zh-Hant",
        )
    assert len(db.rows("text_template_binding")) == 1
    assert read_binding(db, binding.id, {}) == binding
    if translated:
        assert result.rendered is not None
        assert (
            result.rendered.text,
            result.rendered.origin,
            result.rendered.low_confidence,
        ) == ("仮😀2枚", "machine", True)
        identifier, revision = result.rendered.identity()
        assert db.rows("translation")[0].values["revision"] == revision
        assert (
            db.rows("translation_selection")[0].values["translation_id"] == identifier
        )
        assert (
            len(
                read_owned_render_occurrences(
                    db, identifier, {}, expected=result.rendered
                )
            )
            == 1
        )
    else:
        assert result.rendered is None
        assert result.issues == ("missing_template_translation",)
        assert db.rows("translation_selection") == ()


def test_incomplete_field_does_not_activate_its_partial_bindings(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, binding = stored
    partial = compiled_field(db, frame, binding)
    partial = CompiledField(
        partial.source, partial.field, (None,), (False,), ("unmatched_source_frame",)
    )
    with pytest.raises(ValueError, match="incomplete source field"), db.transaction():
        render_field(db, partial, Renderer({}, {}, {}), {}, "zh-Hant")
    assert db.rows("text_template_binding") == ()


def test_name_with_digits_has_one_whole_reference_in_source_and_translation(  # ruff: ignore[too-many-locals] -- assert independently compiled source and target ranges, not a string search
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, initial = stored
    raw = "『仮２』を確認する。"
    checksum = digest(raw.encode())
    source = initial.source.model_copy(
        update={
            "source_unit_id": "t:ja:" + checksum[7:23],
            "source_hash": checksum[7:],
            "source_ref": initial.source.source_ref.model_copy(
                update={
                    "text_hash": checksum[7:],
                    "batch_id": "sha256:" + "a" * 64,
                    "source_version_id": "src:v1:" + "b" * 64,
                }
            ),
        }
    )
    field = normalize_source(raw, source)
    engine = classifier((Term("term:name.synthetic", "card_name", "仮２"),))
    frame, binding = engine.recognize(raw, source, field.parts[0]).bind(
        source, field.parts[0]
    )
    assert len(frame.leaf_schema.slots) == 1
    target = Target.model_validate_json(
        canonical(
            {
                "format": 1,
                "nodes": [
                    {"kind": "Literal", "text": "查看"},
                    {"kind": "LeafRef", "slot": "leaf_0"},
                    {"kind": "Literal", "text": "。"},
                ],
            }
        )
    )
    reference = CardNameReference(kind="card_name", term_id="term:name.synthetic")
    label = Label(reference, "zh-Hant", "譯名１２", "project", False, True)
    renderer = Renderer(
        {(canonical(reference.model_dump(mode="json")), "zh-Hant"): label},
        {},
        {},
        domains=engine.domains,
    )
    with db.transaction():
        db.delete("translation_use", {"id": "use:synthetic"})
        db.insert(
            "text_unit",
            {
                "id": source.source_unit_id,
                "lang": "ja",
                "text": raw,
                "content_hash": checksum,
            },
        )
        db.update(
            "face_revision",
            {"id": "revision:synthetic"},
            {"effect_unit_id": source.source_unit_id},
        )
        db.insert(
            "glossary_term",
            {
                "id": reference.term_id,
                "category": "card_name",
                "concept_key": "name.synthetic",
                "source_ja": "仮２",
                "emphasis": None,
                "authored_source_id": "source",
                "record_key": "synthetic-name",
                "origin": "project",
                "low_confidence": False,
            },
        )
        write_frame(
            db,
            FrameRecord(kind="sentence_template", data=frame),
            field.parts[0].canonical_source,
            "source",
        )
        write_target(
            db,
            TargetRecord.model_validate_json(
                canonical(
                    {
                        "kind": "template_translation",
                        "data": {
                            "template_id": frame.id,
                            "lang": "zh-Hant",
                            "target": target.model_dump(mode="json"),
                        },
                    }
                )
            ),
            "source",
        )
        result = render_field(
            db,
            compiled_field(db, frame, binding),
            renderer,
            {(frame.id, "zh-Hant"): SelectedTarget(target)},
            "zh-Hant",
        )
    assert result.rendered is not None
    assert result.rendered.text == "查看譯名１２。"
    original = read_annotation(
        db, str(db.rows("translation_use_annotation")[0].values["annotation_set_id"])
    )
    translated = read_annotation(
        db, str(db.rows("translation_annotation")[0].values["annotation_set_id"])
    )
    assert len(original.occurrences) == 1
    assert len(translated.occurrences) == 1
    assert original.occurrences[0].ranges[0].model_dump() == {"start": 1, "end": 3}
    assert translated.occurrences[0].ranges[0].model_dump() == {"start": 2, "end": 6}
    assert original.occurrences[0].reference == reference
    assert translated.occurrences[0].reference == reference
    assert original.occurrences[0].bold is True
    assert translated.occurrences[0].bold is True


def test_shared_render_keeps_each_owner_binding_and_verifies_each_output_path(  # ruff: ignore[too-many-locals] -- two independently compiled owners must share only their presentation identity
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, initial = stored
    raw = "  "
    checksum = digest(raw.encode())
    source = initial.source.model_copy(
        update={
            "source_unit_id": "t:ja:" + checksum[7:23],
            "source_hash": checksum[7:],
            "source_ref": initial.source.source_ref.model_copy(
                update={
                    "text_hash": checksum[7:],
                    "batch_id": "sha256:" + "a" * 64,
                    "source_version_id": "src:v1:" + "b" * 64,
                }
            ),
        }
    )
    field = normalize_source(raw, source)
    engine = classifier()
    frame, first = engine.recognize(raw, source, field.parts[0]).bind(
        source, field.parts[0]
    )
    target = Target.model_validate_json(
        canonical({"format": 1, "nodes": [{"kind": "LeafRef", "slot": "leaf_0"}]})
    )
    renderer = Renderer({}, {}, {}, domains=engine.domains)
    with db.transaction():
        db.delete("translation_use", {"id": "use:synthetic"})
        db.insert(
            "text_unit",
            {
                "id": source.source_unit_id,
                "lang": "ja",
                "text": raw,
                "content_hash": checksum,
            },
        )
        db.update(
            "face_revision",
            {"id": "revision:synthetic"},
            {"effect_unit_id": source.source_unit_id},
        )
        write_frame(
            db,
            FrameRecord(kind="sentence_template", data=frame),
            field.parts[0].canonical_source,
            "source",
        )
        write_target(
            db,
            TargetRecord.model_validate_json(
                canonical(
                    {
                        "kind": "template_translation",
                        "data": {
                            "template_id": frame.id,
                            "lang": "zh-Hant",
                            "target": target.model_dump(mode="json"),
                        },
                    }
                )
            ),
            "source",
        )
        targets = {(frame.id, "zh-Hant"): SelectedTarget(target)}
        first_result = render_field(
            db, compiled_field(db, frame, first), renderer, targets, "zh-Hant"
        )
        row = dict(
            db.select(
                "face_revision",
                db.columns("face_revision"),
                where={"id": "revision:synthetic"},
            )[0].values
        )
        db.insert("face_revision", row | {"id": "revision:second", "revision": 2})
        other = source.model_copy(
            update={
                "owner": source.owner.model_copy(
                    update={"revision_id": "revision:second"}
                )
            }
        )
        second_field = normalize_source(raw, other)
        second_frame, second = engine.recognize(raw, other, second_field.parts[0]).bind(
            other, second_field.parts[0]
        )
        assert second_frame == frame
        second_result = render_field(
            db, compiled_field(db, frame, second), renderer, targets, "zh-Hant"
        )
    assert first_result.rendered is not None
    assert second_result.rendered is not None
    assert first_result.rendered.identity() == second_result.rendered.identity()
    assert first.id != second.id
    identifier, _ = first_result.rendered.identity()
    assert len(db.rows("translation")) == 1
    assert len(db.rows("translation_binding")) == 2
    assert (
        len(
            read_owned_render_occurrences(
                db, identifier, engine.domains, expected=first_result.rendered
            )
        )
        == 1
    )
    assert (
        len(
            read_owned_render_occurrences(
                db, identifier, engine.domains, expected=second_result.rendered
            )
        )
        == 1
    )
