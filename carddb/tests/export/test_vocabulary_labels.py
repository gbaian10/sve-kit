"""Vocabulary labels follow resolved choices through the normal export closure."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import (
    Capability,
    Column,
    Json,
    Kind,
    compile_schema,
    create_database,
)
from sve_carddb.build.registry import Registry
from sve_carddb.build.t1 import REGISTRY
from sve_carddb.build.translation_evidence import TABLES
from sve_carddb.core.json import array, digest, object_value, parse
from sve_carddb.domains.translations.direct import write
from sve_carddb.export.media import prepare_media
from sve_carddb.export.reader import read_snapshot
from sve_carddb.export.transport import Ownership, export_snapshot

from ..support.snapshot_project_fixtures import TEXT, populate, schema
from .test_snapshot_export import BATCH
from .test_snapshot_project import projected

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build import Database
    from sve_carddb.export.project import Projection


@pytest.fixture
def vocabulary_db() -> Iterator[Database]:
    base = schema()
    tables = tuple(
        replace(
            t, columns=(*t.columns, Column("low_confidence", Kind.BOOL, nullable=True))
        )
        if t.name == "glossary_term"
        else t
        for t in base.tables
    ) + tuple(t for t in TABLES if t.name == "glossary_translation")
    defined = {t.name for t in tables}
    reserved = tuple(
        Capability(
            capability.name,
            tuple(name for name in capability.tables if name not in defined),
            implemented=False,
        )
        for capability in REGISTRY.capabilities
        if any(name not in defined for name in capability.tables)
    )
    compiled = compile_schema(
        Registry(
            tables, (Capability("synthetic", tuple(t.name for t in tables)), *reserved)
        ),
        ("synthetic",),
        {name: parse(value.encode()) for name, value in base.json_schemas},
    )
    with create_database(compiled) as db:
        with db.transaction():
            populate(db)
            for code in ("elf", "cutthroat", "new_year"):
                db.insert(
                    "vocabulary",
                    {
                        "kind": "trait",
                        "code": code,
                        "label_unit_id": TEXT,
                        "active": True,
                    },
                )
            for code in ("token", "evolve", "advance"):
                db.insert(
                    "vocabulary",
                    {
                        "kind": "special_kind",
                        "code": code,
                        "label_unit_id": TEXT,
                        "active": True,
                    },
                )
            for concept in ("elf", "killer"):
                db.insert(
                    "glossary_term",
                    {
                        "id": "term:trait." + concept,
                        "category": "trait",
                        "source_ja": "合成族群",
                        "concept_key": "trait." + concept,
                        "decision_id": "decision",
                        "low_confidence": concept == "killer",
                    },
                )
                db.insert(
                    "glossary_translation",
                    {
                        "term_id": "term:trait." + concept,
                        "lang": "zh-Hant",
                        "text": "合成精靈" if concept == "elf" else "合成殺手",
                        "origin": "machine" if concept == "elf" else "official",
                        "low_confidence": concept == "elf",
                        "source_id": None if concept == "elf" else "source",
                        "authored_source_id": "source",
                        "record_key": concept,
                    },
                )
        yield db


def label(result: Projection, code: str) -> tuple[object, object, object, object]:
    vocabulary = next(
        r
        for r in result.tables["vocabulary"]
        if r["kind"] == "trait" and r["code"] == code
    )
    binding = object_value(array(vocabulary["translations"])[0])
    translation = next(
        r for r in result.tables["translation"] if r["id"] == binding["translation_id"]
    )
    text = next(
        r["text"]
        for r in result.tables["text_unit"]
        if r["id"] == translation["text_unit_id"]
    )
    assert translation["source_unit_id"] == vocabulary["label_unit_id"]
    assert binding["source"] == {
        "owner": {"kind": "vocabulary", "vocabulary_kind": "trait", "code": code},
        "field": "label",
        "ordinal": None,
    }
    return (
        text,
        translation["origin"],
        translation["authority"],
        translation["low_confidence"],
    )


def test_resolved_authored_and_source_choices_keep_quality_and_update(
    vocabulary_db: Database,
) -> None:
    before = projected(vocabulary_db)
    assert label(before, "elf") == ("合成精靈", "machine", "unofficial", True)
    assert label(before, "cutthroat") == (
        "合成殺手",
        "official",
        "digital_official",
        True,
    )
    with vocabulary_db.transaction():
        record = next(r.values for r in vocabulary_db.rows("source_record"))
        vocabulary_db.insert("source_record", {**record, "id": "next_revision"})
        vocabulary_db.update(
            "glossary_translation",
            {"term_id": "term:trait.elf", "lang": "zh-Hant"},
            {"authored_source_id": "next_revision"},
        )
    assert (
        projected(vocabulary_db).tables["translation"] == before.tables["translation"]
    )
    with vocabulary_db.transaction():
        vocabulary_db.update(
            "glossary_translation",
            {"term_id": "term:trait.elf", "lang": "zh-Hant"},
            {"text": "改選精靈"},
        )
    assert label(projected(vocabulary_db), "elf")[0] == "改選精靈"


def test_absent_choice_keeps_nonempty_original(vocabulary_db: Database) -> None:
    result = projected(vocabulary_db)
    vocabulary = next(r for r in result.tables["vocabulary"] if r["code"] == "new_year")
    assert vocabulary["translations"] == []
    assert next(
        r["text"]
        for r in result.tables["text_unit"]
        if r["id"] == vocabulary["label_unit_id"]
    )


def test_official_english_label_and_chinese_choice_survive_snapshot_reader(
    vocabulary_db: Database,
) -> None:
    with vocabulary_db.transaction():
        vocabulary_db.insert(
            "language",
            {"code": "en", "display_name": "English", "fallback_order": Json(["ja"])},
        )
        vocabulary_db.update(
            "text_unit", {"id": TEXT}, {"content_hash": digest(b"Synthetic text")}
        )
        write(
            vocabulary_db,
            {"vocabulary_kind": "trait", "vocabulary_code": "elf"},
            field="label",
            lang="en",
            source_unit_id=TEXT,
            text="Synthetic pixie",
            origin="official",
            source_id="source",
            low_confidence=False,
        )
    projection = prepare_media(projected(vocabulary_db), None, revision=1).projection
    snapshot = export_snapshot(
        projection, Ownership.from_database(vocabulary_db, projection), BATCH
    )
    tables = read_snapshot(
        snapshot.manifest, {key: blob.raw for key, blob in snapshot.payloads.items()}
    )
    vocabulary = next(
        row
        for row in tables["vocabulary"]
        if (row["kind"], row["code"]) == ("trait", "elf")
    )
    labels = {}
    for raw in array(vocabulary["translations"]):
        binding = object_value(raw)
        translation = next(
            row
            for row in tables["translation"]
            if row["id"] == binding["translation_id"]
        )
        text = next(
            row["text"]
            for row in tables["text_unit"]
            if row["id"] == translation["text_unit_id"]
        )
        labels[binding["target_lang"]] = (
            text,
            translation["origin"],
            translation["authority"],
        )
    assert labels == {
        "en": ("Synthetic pixie", "official", "sve_official"),
        "zh-Hant": ("合成精靈", "machine", "unofficial"),
    }
    assert tables == projection.tables


def test_badge_does_not_borrow_a_marker_object_name(vocabulary_db: Database) -> None:
    with vocabulary_db.transaction():
        for concept in ("token", "evolve_marker", "advance_marker"):
            vocabulary_db.insert(
                "glossary_term",
                {
                    "id": "term:object." + concept,
                    "category": "rule_term",
                    "source_ja": "合成物件",
                    "concept_key": "object." + concept,
                    "decision_id": "decision",
                    "low_confidence": False,
                },
            )
            vocabulary_db.insert(
                "glossary_translation",
                {
                    "term_id": "term:object." + concept,
                    "lang": "zh-Hant",
                    "text": "合成衍生物" if concept == "token" else "合成標記物件",
                    "origin": "project",
                    "low_confidence": False,
                    "source_id": None,
                    "authored_source_id": "source",
                    "record_key": concept,
                },
            )
    result = projected(vocabulary_db)
    for code in ("evolve", "advance"):
        assert (
            next(
                r
                for r in result.tables["vocabulary"]
                if r["kind"] == "special_kind" and r["code"] == code
            )["translations"]
            == []
        )
    token = next(
        r
        for r in result.tables["vocabulary"]
        if r["kind"] == "special_kind" and r["code"] == "token"
    )
    binding = object_value(array(token["translations"])[0])
    translation = next(
        r for r in result.tables["translation"] if r["id"] == binding["translation_id"]
    )
    assert (
        next(
            r["text"]
            for r in result.tables["text_unit"]
            if r["id"] == translation["text_unit_id"]
        )
        == "合成衍生物"
    )
