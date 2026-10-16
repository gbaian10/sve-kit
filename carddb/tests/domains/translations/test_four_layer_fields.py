"""Whole fields persist typed bindings, exact selections and owner-specific output paths."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json
from sve_carddb.contracts.four_layer import (
    CardArgs,
    CardNameReference,
    CardNP,
    SlotRef,
    Target,
)
from sve_carddb.contracts.source_binding import LeafOccurrence
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.translations.four_layer_analysis import SourceAnalysis
from sve_carddb.domains.translations.four_layer_authored import (
    FrameRecord,
    TargetRecord,
)
from sve_carddb.domains.translations.four_layer_classification import (
    Classifier,
    Recognized,
    Term,
)
from sve_carddb.domains.translations.four_layer_derivation import (
    DerivedField,
    derive_source,
)
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
    write_frame,
    write_target,
)

from .test_four_layer_classification import classifier, source
from .test_four_layer_n1 import n1_classifier
from .test_four_layer_normalizer import rekey
from .test_four_layer_storage import compiled as compiled  # ruff: ignore[useless-import-alias] -- shared actual SQLite fixture
from .test_four_layer_storage import default_target
from .test_four_layer_storage import stored as stored  # ruff: ignore[useless-import-alias] -- shared exact source fixture

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding


def compiled_field(
    db: Database, frame: Frame, binding: SourceBinding, engine: Classifier | None = None
) -> CompiledField:
    text = db.select(
        "text_unit", ("text",), where={"id": binding.source.source_unit_id}
    )[0].values["text"]
    assert isinstance(text, str)
    if engine is None:
        # Storage fixtures enter after compilation with their synthetic Nat/count role.
        field = normalize_source(text, binding.source)
        derived = DerivedField(
            SourceAnalysis(
                field,
                (
                    Recognized(
                        frame.leaf_schema,
                        binding.values,
                        binding.occurrences,
                        (),
                        False,
                    ),
                ),
                ((),),
                ((),),
                field,
                ((),),
            ),
            (None,),
        )
    else:
        derived = derive_source(text, binding.source, engine)
    return CompiledField(
        CardSource(binding.source, text, "card", "face", "source"),
        derived,
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
    assert db.rows("text_template_binding")[0].values["source"] == Json(
        binding.source.model_dump(mode="json")
    )
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
                db.select(
                    "render_leaf_occurrence",
                    db.columns("render_leaf_occurrence"),
                    where={"translation_id": identifier, "binding_id": binding.id},
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
        partial.source,
        partial.derived,
        (None,),
        (False,),
        ("unmatched_source_frame",),
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
            frame=frame,
            forms={},
        )
        result = render_field(
            db,
            compiled_field(db, frame, binding, engine),
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
            frame=frame,
            forms={},
        )
        targets = {(frame.id, "zh-Hant"): SelectedTarget(target)}
        first_result = render_field(
            db, compiled_field(db, frame, first, engine), renderer, targets, "zh-Hant"
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
            db, compiled_field(db, frame, second, engine), renderer, targets, "zh-Hant"
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
            db.select(
                "render_leaf_occurrence",
                db.columns("render_leaf_occurrence"),
                where={"translation_id": identifier, "binding_id": first.id},
            )
        )
        == 1
    )
    assert (
        len(
            db.select(
                "render_leaf_occurrence",
                db.columns("render_leaf_occurrence"),
                where={"translation_id": identifier, "binding_id": second.id},
            )
        )
        == 1
    )


def test_whole_field_rejects_a_forged_omission_on_an_explicit_number(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, binding = stored
    omitted = LeafOccurrence(
        slot="n",
        ordinal=1,
        raw_spans=(),
        canonical_spans=(),
        source_unit=None,
        source_presence="omitted",
        resolution_rule="fixture.antecedent.v1",
    )
    mixed = rekey(
        binding.model_copy(update={"occurrences": (*binding.occurrences, omitted)})
    )
    with db.transaction():
        db.delete("translation_use", {"id": "use:synthetic"})
        with pytest.raises(ValueError, match="cannot have an omitted occurrence"):
            render_field(
                db,
                compiled_field(db, frame, mixed),
                Renderer({}, {}, {}),
                {},
                "zh-Hant",
            )
    assert db.rows("text_template_binding") == ()


def test_valid_binding_hash_does_not_authorize_a_forged_numeric_value(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, binding = stored
    forged = rekey(binding.model_copy(update={"values": {"n": 9}}))
    with db.transaction():
        db.delete("translation_use", {"id": "use:synthetic"})
        with pytest.raises(ValueError, match="Numeric leaf value differs"):
            render_field(
                db,
                compiled_field(db, frame, forged),
                Renderer({}, {}, {}),
                {},
                "zh-Hant",
            )
    assert db.rows("text_template_binding") == ()


def test_whole_field_rejects_a_binding_from_another_owner_with_equal_bytes(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, binding = stored
    compiled = compiled_field(db, frame, binding)
    borrowed = rekey(
        binding.model_copy(
            update={
                "source": binding.source.model_copy(
                    update={
                        "owner": binding.source.owner.model_copy(
                            update={"revision_id": "revision:other"}
                        )
                    }
                )
            }
        )
    )
    compiled = replace(compiled, matches=(Matched(frame, borrowed),))
    with (
        pytest.raises(ValueError, match="another owner source descriptor"),
        db.transaction(),
    ):
        render_field(db, compiled, Renderer({}, {}, {}), {}, "zh-Hant")
    assert db.rows("text_template_binding") == ()


def test_whole_field_rechecks_a_mutated_binding_value_before_writing(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, binding = stored
    compiled = compiled_field(db, frame, binding)
    binding.values["n"] = 9
    with pytest.raises(ValueError, match="Binding identity"), db.transaction():
        render_field(db, compiled, Renderer({}, {}, {}), {}, "zh-Hant")
    assert db.rows("text_template_binding") == ()


def test_suppression_still_rejects_a_target_borrowing_another_source_operand(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, _ = stored
    raw = "自分の墓場のフォロワー７枚を選ぶ。相手の墓場のフォロワー７枚を選ぶ。"
    engine = n1_classifier()
    descriptor = source(raw)
    derived = derive_source(raw, descriptor, engine)
    frame, binding = derived.bind(0)
    first, second = frame.leaf_schema.slots[:4], frame.leaf_schema.slots[4:8]
    borrowed = CardNP(
        kind="NP",
        constructor="CardNP",
        args=CardArgs(
            kind=SlotRef(slot=first[2].name),
            quantity=SlotRef(slot=first[3].name),
            owner=SlotRef(slot=second[0].name),
            zone=SlotRef(slot=first[1].name),
        ),
    )
    compiled = CompiledField(
        CardSource(descriptor, raw, "card", "face", "source"),
        derived,
        (Matched(frame, binding),),
        (False,),
        (),
    )
    selected = SelectedTarget(Target(format=1, nodes=(borrowed,)))
    with (
        pytest.raises(ValueError, match="one supported source operand"),
        db.transaction(),
    ):
        render_field(
            db,
            compiled,
            Renderer({}, {}, {}, domains=engine.domains),
            {(frame.id, "zh-Hant"): selected},
            "zh-Hant",
            suppress=True,
        )
    assert db.rows("text_template_binding") == ()
