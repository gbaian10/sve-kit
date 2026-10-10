"""Actual SQLite rows preserve source arrays and reject corrupt cross-row semantics."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from jsonschema import ValidationError

from sve_carddb.build import (
    Json,
    compile_schema,
    create_database,
    four_layer,
    templates,
)
from sve_carddb.build.t0_json import schemas as t0_schemas
from sve_carddb.build.t1 import REGISTRY
from sve_carddb.build.t1_json import schemas as t1_schemas
from sve_carddb.contracts.annotations import AnnotationSet
from sve_carddb.contracts.four_layer import hash_payload
from sve_carddb.contracts.source_binding import LeafOccurrence
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.translations.four_layer_authored import (
    FormRecord,
    FrameRecord,
    TargetRecord,
)
from sve_carddb.domains.translations.four_layer_storage import (
    read_annotation,
    read_binding,
    read_forms,
    read_frame,
    read_render_occurrences,
    read_target,
    write_annotation,
    write_binding,
    write_form,
    write_frame,
    write_target,
)

from ...support.build_db_fixtures import seed
from .test_four_layer_normalizer import bound_number, rekey

if TYPE_CHECKING:
    from collections.abc import Iterator

    from pydantic import JsonValue

    from sve_carddb.build import CompiledSchema, Database, Value
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding


@pytest.fixture(scope="session")
def compiled() -> CompiledSchema:
    old = {t.name for t in templates.TABLES}
    registry = replace(
        REGISTRY,
        tables=tuple(t for t in REGISTRY.tables if t.name not in old)
        + four_layer.TABLES,
        capabilities=tuple(
            replace(cap, tables=tuple(t.name for t in four_layer.TABLES))
            if cap.name == "translation_templates"
            else cap
            for cap in REGISTRY.capabilities
        ),
    )
    return compile_schema(
        registry,
        ("translation_templates",),
        t0_schemas()
        | t1_schemas()
        | four_layer.schemas()
        | {"TranslationTokens": {"type": "null"}},
        version=7,
    )


@pytest.fixture
def stored(compiled: CompiledSchema) -> Iterator[tuple[Database, Frame, SourceBinding]]:
    raw = "仮😀２枚"
    field, frame, binding = bound_number(raw)
    with create_database(compiled) as db:
        seed(db)
        with db.transaction():
            db.insert(
                "language",
                {
                    "code": "zh-Hant",
                    "display_name": "合成語言",
                    "fallback_order": Json([]),
                },
            )
            db.insert(
                "text_unit",
                {
                    "id": binding.source.source_unit_id,
                    "lang": "ja",
                    "text": raw,
                    "content_hash": "sha256:" + binding.source.source_hash,
                },
            )
            revision = dict(db.rows("face_revision")[0].values)
            revision.update(
                id="revision:synthetic",
                revision=1,
                effect_unit_id=binding.source.source_unit_id,
            )
            db.insert("face_revision", revision)
            write_frame(
                db,
                FrameRecord(kind="sentence_template", data=frame),
                field.parts[0].canonical_source,
                "source",
            )
            db.insert(
                "translation_context",
                {
                    "id": "context:synthetic",
                    "source_unit_id": binding.source.source_unit_id,
                    "semantic_variant": "variant:synthetic",
                },
            )
            use: dict[str, Value] = dict.fromkeys(db.columns("translation_use"))
            use.update(
                id="use:synthetic",
                context_id="context:synthetic",
                field="effect",
                face_revision_id="revision:synthetic",
            )
            db.insert("translation_use", use)
            selected = TargetRecord.model_validate_json(
                canonical(
                    {
                        "kind": "template_translation",
                        "data": {
                            "template_id": frame.id,
                            "lang": "zh-Hant",
                            "target": {
                                "format": 1,
                                "nodes": [
                                    {"kind": "Literal", "text": "仮😀"},
                                    {"kind": "LeafRef", "slot": "n"},
                                    {"kind": "Literal", "text": "枚"},
                                ],
                            },
                        },
                    }
                )
            )
            write_target(db, selected, "source")
        yield db, frame, binding


def test_frame_target_and_owner_binding_round_trip_through_actual_sqlite(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, binding = stored
    assert read_frame(db, frame.id).frame == frame
    assert read_target(db, frame.id, "zh-Hant", "default").nodes[1].kind == "LeafRef"
    with db.transaction():
        write_binding(db, "use:synthetic", binding, {})
    assert read_binding(db, binding.id, {}) == binding


def test_explicit_then_omitted_occurrences_keep_their_array_order(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, binding = stored
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
        write_binding(db, "use:synthetic", mixed, {})
    assert read_binding(db, mixed.id, {}) == mixed


def test_binding_cannot_move_to_a_use_with_another_owner(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, binding = stored
    with db.transaction():
        use = dict(db.rows("translation_use")[0].values)
        use.update(
            id="use:other",
            face_revision_id=None,
            printing_id="printing",
            face_id="face",
        )
        db.insert("translation_use", use)
    with pytest.raises(ValueError, match="exact use owner"), db.transaction():
        write_binding(db, "use:other", binding, {})
    assert db.rows("text_template_binding") == ()


@pytest.mark.parametrize("column", ["content_hash", "interface_key"])
def test_frame_identity_and_interface_are_rechecked_on_read(
    stored: tuple[Database, Frame, SourceBinding], column: str
) -> None:
    db, frame, _ = stored
    with db.transaction():
        db.update("sentence_template", {"id": frame.id}, {column: "0" * 64})
    with pytest.raises(ValueError, match=r"stored four-layer frame|interface key"):
        read_frame(db, frame.id)


def test_valid_json_cannot_hide_required_leaves_in_literal_target(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, _ = stored
    with db.transaction():
        db.update(
            "template_translation",
            {"template_id": frame.id, "lang": "zh-Hant", "variant_key": "default"},
            {
                "target": Json(
                    {"format": 1, "nodes": [{"kind": "Literal", "text": "{{n}}"}]}
                )
            },
        )
    with pytest.raises(ValueError, match="Invalid stored four-layer target"):
        read_target(db, frame.id, "zh-Hant", "default")


@pytest.mark.parametrize(
    "corrupt", ["boolean_value", "missing_leaf", "position_gap", "stale_text"]
)
def test_typed_binding_read_rejects_corrupt_relational_values(
    stored: tuple[Database, Frame, SourceBinding], corrupt: str
) -> None:
    db, _, binding = stored
    with db.transaction():
        write_binding(db, "use:synthetic", binding, {})
    with db.transaction():
        if corrupt == "boolean_value":
            db.update(
                "text_template_binding",
                {"id": binding.id},
                {"values": Json({"n": True})},
            )
        elif corrupt == "missing_leaf":
            db.delete(
                "binding_leaf_occurrence",
                {"binding_id": binding.id, "slot": "n", "ordinal": 0},
            )
        elif corrupt == "position_gap":
            db.update(
                "binding_leaf_occurrence",
                {"binding_id": binding.id, "slot": "n", "ordinal": 0},
                {"position": 2},
            )
        else:
            db.update(
                "text_unit",
                {"id": binding.source.source_unit_id},
                {"text": "新版合成文字"},
            )
    with pytest.raises(
        ValueError,
        match=r"stored four-layer binding|positions must be continuous|stale exact bytes",
    ):
        read_binding(db, binding.id, {})


def test_nested_json_extra_fields_are_rejected_before_insert(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, _ = stored
    with pytest.raises(ValidationError), db.transaction():
        db.update(
            "sentence_template",
            {"id": frame.id},
            {
                "source": Json(
                    frame.source.model_dump(mode="json") | {"private_override": True}
                )
            },
        )


def test_pinned_form_rule_and_typed_signature_survive_sqlite(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, _ = stored
    with db.transaction():
        form = FormRecord.model_validate_json(
            canonical(
                {
                    "kind": "translation_form",
                    "data": {
                        "id": "keyword.display",
                        "lang": "zh-Hant",
                        "rule": "keyword.display.v1",
                        "signature": [
                            {"name": "term", "type": "Concept", "role": "keyword"}
                        ],
                        "cases": {"default": [{"kind": "Label", "arg": "term"}]},
                    },
                }
            )
        )
        write_form(db, form, "source")
    assert read_forms(db)["keyword.display", "zh-Hant"].signature[0].type == "Concept"
    with db.transaction():
        db.update(
            "translation_form",
            {"id": "keyword.display", "lang": "zh-Hant"},
            {"rule": "unknown.rule.v1"},
        )
    with pytest.raises(ValueError, match="Invalid stored four-layer form"):
        read_forms(db)


def test_render_leaf_source_links_and_unicode_bounds_are_checked(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, binding = stored
    with db.transaction():
        write_binding(db, "use:synthetic", binding, {})
        db.insert(
            "translation",
            {
                "id": "tr:synthetic",
                "context_id": "context:synthetic",
                "target_lang": "zh-Hant",
                "revision": 0,
                "text": "仮😀2枚",
                "tokens": None,
                "origin": "project",
                "authority": "unofficial",
                "low_confidence": False,
                "source_hash": "sha256:" + binding.source.source_hash,
                "source_id": None,
            },
        )
        db.insert(
            "render_leaf_occurrence",
            {
                "translation_id": "tr:synthetic",
                "binding_id": binding.id,
                "node_path": Json([1]),
                "slot": "n",
                "source_ordinals": Json([0]),
                "ranges": Json([{"start": 2, "end": 3}]),
            },
        )
    assert read_render_occurrences(db, "tr:synthetic", {})[0].ranges[0].start == 2
    key: dict[str, Value] = {
        "translation_id": "tr:synthetic",
        "binding_id": binding.id,
        "node_path": Json([1]),
    }
    with db.transaction():
        db.update("render_leaf_occurrence", key, {"source_ordinals": Json([1])})
    with pytest.raises(ValueError, match="missing source occurrences"):
        read_render_occurrences(db, "tr:synthetic", {})
    with db.transaction():
        db.update(
            "render_leaf_occurrence",
            key,
            {"source_ordinals": Json([0]), "ranges": Json([{"start": 2, "end": 20}])},
        )
    with pytest.raises(ValueError, match="exceeds exact translation text"):
        read_render_occurrences(db, "tr:synthetic", {})


def annotation(reference: JsonValue, bold: bool | None = True) -> AnnotationSet:
    """The oracle recipe is independent of storage and uses a non-BMP target scalar."""
    text = "自編😀詞"
    data: dict[str, JsonValue] = {
        "text_unit_id": "t:zh-Hant:" + digest(text.encode())[7:23],
        "occurrences": [
            {
                "ordinal": 0,
                "reference": reference,
                "ranges": [{"start": 3, "end": 4}],
                "bold": bold,
            }
        ],
    }
    checksum = hash_payload({"recipe": "annotation-v1", **data})
    return AnnotationSet.model_validate_json(
        canonical({"id": "ann:" + checksum, **data})
    )


def test_annotation_writes_and_reads_require_exact_text_and_public_reference(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, _ = stored
    selected = annotation({"kind": "vocabulary", "key": ["type", "follower"]})
    with db.transaction():
        db.insert(
            "text_unit",
            {
                "id": selected.text_unit_id,
                "lang": "zh-Hant",
                "text": "自編😀詞",
                "content_hash": digest("自編😀詞".encode()),
            },
        )
        write_annotation(db, selected)
    assert read_annotation(db, selected.id) == selected
    wrong_bold = annotation({"kind": "vocabulary", "key": ["type", "follower"]}, False)
    with pytest.raises(ValueError, match="must be bold"), db.transaction():
        write_annotation(db, wrong_bold)
    with db.transaction():
        db.update("text_unit", {"id": selected.text_unit_id}, {"text": "相同長度詞"})
    with pytest.raises(ValueError, match="Invalid stored annotation set"):
        read_annotation(db, selected.id)


def test_annotation_concept_category_and_current_emphasis_are_checked(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, _ = stored
    selected = annotation({"kind": "glossary", "key": "term:fixture.keyword"})
    with db.transaction():
        db.insert(
            "text_unit",
            {
                "id": selected.text_unit_id,
                "lang": "zh-Hant",
                "text": "自編😀詞",
                "content_hash": digest("自編😀詞".encode()),
            },
        )
        db.insert(
            "glossary_term",
            {
                "id": "term:fixture.keyword",
                "category": "keyword",
                "concept_key": "fixture.keyword",
                "source_ja": "合成語",
                "emphasis": None,
                "authored_source_id": "source",
                "record_key": "synthetic-term",
                "origin": "project",
                "low_confidence": False,
            },
        )
        write_annotation(db, selected)
    assert read_annotation(db, selected.id) == selected
    with db.transaction():
        db.update(
            "glossary_term", {"id": "term:fixture.keyword"}, {"category": "card_name"}
        )
    with pytest.raises(ValueError, match="Invalid stored annotation set"):
        read_annotation(db, selected.id)
    with db.transaction():
        db.update(
            "glossary_term",
            {"id": "term:fixture.keyword"},
            {"category": "rule_term", "emphasis": None},
        )
    with pytest.raises(ValueError, match="Invalid stored annotation set"):
        read_annotation(db, selected.id)


def test_valid_binding_hash_does_not_authorize_a_forged_numeric_value(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, binding = stored
    forged = rekey(binding.model_copy(update={"values": {"n": 9}}))
    with (
        pytest.raises(ValueError, match="Numeric leaf value differs"),
        db.transaction(),
    ):
        write_binding(db, "use:synthetic", forged, {})
    assert db.rows("text_template_binding") == ()
