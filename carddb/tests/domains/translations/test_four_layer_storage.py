"""Actual SQLite rows preserve source arrays and reject corrupt cross-row semantics."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from jsonschema import ValidationError

from sve_carddb.build import Json, compile_schema, create_database, four_layer
from sve_carddb.build.model import Column, Kind, Table
from sve_carddb.build.t0_json import schemas as t0_schemas
from sve_carddb.build.t1 import REGISTRY
from sve_carddb.build.t1_json import schemas as t1_schemas
from sve_carddb.build.t1_rulings import schemas as ruling_schemas
from sve_carddb.contracts.annotations import AnnotationSet
from sve_carddb.contracts.four_layer import Target, hash_payload
from sve_carddb.contracts.source_binding import SourceDescriptor
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.translations.four_layer_authored import (
    FrameRecord,
    TargetRecord,
)
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.four_layer_storage import (
    payload,
    read_annotation,
    write_annotation,
    write_binding,
    write_frame,
    write_target,
)

from ...support.build_db_fixtures import seed
from .test_four_layer_classification import classifier
from .test_four_layer_classification import source as classified_source
from .test_four_layer_normalizer import bound_number

if TYPE_CHECKING:
    from collections.abc import Iterator

    from pydantic import JsonValue

    from sve_carddb.build import CompiledSchema, Database, Value
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding


@pytest.fixture(scope="session")
def compiled() -> CompiledSchema:
    # Exercise the reserved owner without enabling a production keyword importer.
    old = {t.name for t in four_layer.FRAME_TABLES}
    registry = replace(
        REGISTRY,
        tables=tuple(t for t in REGISTRY.tables if t.name not in old)
        + four_layer.FRAME_TABLES
        + (Table("keyword", (Column("id", Kind.ID),), ("id",)),),
        capabilities=tuple(
            replace(cap, tables=tuple(t.name for t in four_layer.FRAME_TABLES))
            if cap.name == "translation_templates"
            else replace(cap, implemented=True)
            if cap.name == "keyword"
            else cap
            for cap in REGISTRY.capabilities
        ),
    )
    return compile_schema(
        registry,
        ("translation_templates", "qa", "cr", "keyword", "correction", "rulings"),
        t0_schemas()
        | t1_schemas()
        | four_layer.schemas()
        | ruling_schemas()
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
            db.insert("keyword", {"id": "keyword:synthetic"})
            db.insert(
                "qa",
                {
                    "id": "qa",
                    "region": "jp",
                    "official_number": None,
                    "stable_source_key": "synthetic",
                    "source_url": "https://example.invalid/qa",
                },
            )
            db.insert(
                "qa_version",
                {
                    "id": "qa:synthetic",
                    "qa_id": "qa",
                    "revision": 1,
                    "published_on": None,
                    "updated_on": None,
                    "date_raw": None,
                    "observed_at": "2026-10-01T00:00:00Z",
                    "question_unit_id": "text",
                    "answer_unit_id": "text",
                    "state": "active",
                    "source_id": "source",
                    "supersedes_id": None,
                },
            )
            db.insert(
                "cr_version",
                {
                    "id": "cr",
                    "region": "jp",
                    "version": "synthetic",
                    "published_on": None,
                    "effective_on": None,
                    "source_id": "source",
                    "source_url": "https://example.invalid/cr",
                },
            )
            db.insert(
                "cr_clause",
                {
                    "id": "cr:synthetic",
                    "cr_version_id": "cr",
                    "number": "1",
                    "text_unit_id": "text",
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
            write_target(db, selected, "source", frame=frame, forms={})
        yield db, frame, binding


def default_target() -> Target:
    return Target.model_validate_json(
        canonical(
            {
                "format": 1,
                "nodes": [
                    {"kind": "Literal", "text": "仮😀"},
                    {"kind": "LeafRef", "slot": "n"},
                    {"kind": "Literal", "text": "枚"},
                ],
            }
        )
    )


def test_frame_target_and_owner_binding_write_through_actual_sqlite(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, binding = stored
    assert db.rows("template_translation")[0].values["target"] == Json(
        default_target().model_dump(mode="json")
    )
    with db.transaction():
        write_binding(db, "use:synthetic", binding)
    values = payload(db.rows("text_template_binding")[0])
    values.pop("use_id")
    expected = binding.model_dump(mode="json")
    occurrences = expected.pop("occurrences")
    assert values == expected
    leaves = []
    for row in db.rows("binding_leaf_occurrence"):
        leaf = payload(row)
        leaf.pop("binding_id")
        leaf.pop("position")
        leaves.append(leaf)
    assert leaves == occurrences


@pytest.mark.parametrize(
    ("owner", "field", "columns"),
    [
        (
            {"kind": "face_revision", "revision_id": "revision:synthetic"},
            "effect",
            {"face_revision_id": "revision:synthetic"},
        ),
        (
            {"kind": "printing_face", "printing_id": "printing", "face_id": "face"},
            "effect",
            {"printing_id": "printing", "face_id": "face"},
        ),
        (
            {"kind": "qa_version", "qa_version_id": "qa:synthetic"},
            "question",
            {"qa_version_id": "qa:synthetic"},
        ),
        (
            {"kind": "cr_clause", "cr_clause_id": "cr:synthetic"},
            "effect",
            {"cr_clause_id": "cr:synthetic"},
        ),
        (
            {
                "kind": "vocabulary",
                "vocabulary_kind": "type",
                "vocabulary_code": "follower",
            },
            "label",
            {"vocabulary_kind": "type", "vocabulary_code": "follower"},
        ),
        (
            {"kind": "product", "product_id": "product"},
            "label",
            {"product_id": "product"},
        ),
        (
            {"kind": "product_family", "product_family_id": "family"},
            "label",
            {"product_family_id": "family"},
        ),
        (
            {"kind": "keyword", "keyword_id": "keyword:synthetic"},
            "effect",
            {"keyword_id": "keyword:synthetic"},
        ),
    ],
    ids=lambda value: value.get("kind") if isinstance(value, dict) else None,
)
def test_all_owner_kinds_round_trip_bindings(
    stored: tuple[Database, Frame, SourceBinding],
    owner: dict[str, str],
    field: str,
    columns: dict[str, str],
) -> None:
    db, _, _ = stored
    raw = "仮"
    descriptor = SourceDescriptor.model_validate_json(
        canonical(
            classified_source(raw).model_dump(mode="json")
            | {"owner": owner, "field": field}
        )
    )
    normalized = normalize_source(raw, descriptor)
    part = normalized.parts[0]
    engine = classifier((Term("term:synthetic", "rule_term", raw),))
    frame, binding = engine.recognize(raw, descriptor, part).bind(descriptor, part)
    with db.transaction():
        db.delete("translation_use", {"id": "use:synthetic"})
        db.insert(
            "text_unit",
            {
                "id": descriptor.source_unit_id,
                "lang": "ja",
                "text": raw,
                "content_hash": "sha256:" + descriptor.source_hash,
            },
        )
        write_frame(
            db,
            FrameRecord(kind="sentence_template", data=frame),
            part.canonical_source,
            "source",
        )
        db.insert(
            "translation_context",
            {
                "id": "context:owner",
                "source_unit_id": descriptor.source_unit_id,
                "semantic_variant": "variant:owner",
            },
        )
        use: dict[str, Value] = dict.fromkeys(db.columns("translation_use"))
        use.update(id="use:owner", context_id="context:owner", field=field, **columns)
        db.insert("translation_use", use)
        write_binding(db, "use:owner", binding)
    values = payload(db.rows("text_template_binding")[0])
    values.pop("use_id")
    expected = binding.model_dump(mode="json")
    occurrences = expected.pop("occurrences")
    assert values == expected
    leaves = []
    for row in db.rows("binding_leaf_occurrence"):
        leaf = payload(row)
        leaf.pop("binding_id")
        leaf.pop("position")
        leaves.append(leaf)
    assert leaves == occurrences


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
        write_binding(db, "use:other", binding)
    assert db.rows("text_template_binding") == ()


def test_write_frame_rejects_a_forged_content_hash_before_insert(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, _ = stored
    before = db.rows("sentence_template")
    forged = frame.model_copy(
        update={"id": "frame:" + "0" * 64, "content_hash": "0" * 64}
    )
    with pytest.raises(ValueError, match="semantic content hash mismatch"):
        write_frame(
            db,
            FrameRecord(kind="sentence_template", data=forged),
            "仮😀N枚",
            "source",
        )
    assert db.rows("sentence_template") == before


def test_valid_json_cannot_hide_required_leaves_in_literal_target(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, _ = stored
    bad = TargetRecord.model_validate_json(
        canonical(
            {
                "kind": "template_translation",
                "data": {
                    "template_id": frame.id,
                    "lang": "zh-Hant",
                    "target": {
                        "format": 1,
                        "nodes": [{"kind": "Literal", "text": "{{n}}"}],
                    },
                },
            }
        )
    )
    with (
        db.transaction(),
        pytest.raises(ValueError, match=r"[Ll]eaf|[Ll]iteral|[Pp]laceholder"),
    ):
        write_target(db, bad, "source", frame=frame, forms={})


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


def test_target_cannot_use_its_frame_source_language(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, _ = stored
    target = TargetRecord.model_validate_json(
        canonical(
            {
                "kind": "template_translation",
                "data": {
                    "template_id": frame.id,
                    "lang": frame.source.source_lang,
                    "target": default_target().model_dump(mode="json"),
                },
            }
        )
    )
    with (
        pytest.raises(ValueError, match="language must differ"),
        db.transaction(),
    ):
        write_target(db, target, "source", frame=frame, forms={})
    assert len(db.rows("template_translation")) == 1


def test_target_must_belong_to_the_frame_it_is_verified_against(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, _ = stored
    target = TargetRecord.model_validate_json(
        canonical(
            {
                "kind": "template_translation",
                "data": {
                    "template_id": "frame:" + "0" * 64,
                    "lang": "zh-Hant",
                    "target": default_target().model_dump(mode="json"),
                },
            }
        )
    )
    with (
        pytest.raises(ValueError, match="differs from the frame"),
        db.transaction(),
    ):
        write_target(db, target, "source", frame=frame, forms={})


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
