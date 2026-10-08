"""Nullable FK activation, future DSL constraints and independent relation edges."""

import sqlite3
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import (
    Column,
    Kind,
    Table,
    compile_schema,
    create_database,
    rebuild_database,
)
from sve_carddb.build_db.t0_json import schemas as t0_schemas
from sve_carddb.build_db.t1 import MINIMUM_CAPABILITIES, REGISTRY, compile_minimum
from sve_carddb.build_db.t1_json import schemas as t1_schemas

from .build_db_t1_fixtures import populate as populate_a
from .build_db_t1b_fixtures import populate, rows
from .test_build_db_t1b import db as db  # ruff: ignore[useless-import-alias] -- explicitly re-export the pytest fixture
from .test_build_db_t1b import key

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import CompiledSchema, Database


def future_dsl() -> CompiledSchema:
    tables = tuple(
        Table(name, (Column("id", Kind.ID),), ("id",))
        for name in ("dsl_document", "dsl_load")
    )
    registry = replace(
        REGISTRY,
        tables=(*REGISTRY.tables, *tables),
        capabilities=tuple(
            replace(cap, implemented=True) if cap.name == "dsl" else cap
            for cap in REGISTRY.capabilities
        ),
    )
    return compile_schema(
        registry,
        (*MINIMUM_CAPABILITIES, "en", "dsl"),
        t0_schemas() | t1_schemas(),
        version=3,
    )


def test_reskin_dsl_null_check_is_independent_of_disabled_capability() -> None:
    with create_database(future_dsl()) as database:
        with database.transaction():
            populate(database)
            database.insert("dsl_document", {"id": "dsl"})
        with pytest.raises(sqlite3.IntegrityError), database.transaction():
            database.update("card_related", key("card_related"), {"dsl_id": "dsl"})
        with database.transaction():
            database.update(
                "card_related",
                key("card_related"),
                {"relation": "mentions", "source_kind": "dsl", "dsl_id": "dsl"},
            )


def test_divergence_override_requires_document_when_dsl_enabled() -> None:
    with create_database(future_dsl()) as database:
        with database.transaction():
            populate(database)
            database.insert("dsl_document", {"id": "dsl"})
        with pytest.raises(sqlite3.IntegrityError), database.transaction():
            database.update(
                "region_divergence",
                key("region_divergence"),
                {"effect": "override_dsl"},
            )
        with database.transaction():
            database.update(
                "region_divergence",
                key("region_divergence"),
                {"effect": "override_dsl", "override_dsl_id": "dsl"},
            )


@pytest.mark.parametrize(
    ("table", "column"),
    [
        ("card_related", "dsl_id"),
        ("region_divergence", "override_dsl_id"),
    ],
)
def test_unimplemented_dsl_remains_null_only(
    db: Database, table: str, column: str
) -> None:
    with db.transaction():
        db.update("card_related", key("card_related"), {"relation": "mentions"})
    with pytest.raises(sqlite3.IntegrityError), db.transaction():
        db.update(table, key(table), {column: "missing"})


@pytest.mark.parametrize(
    ("table", "column"),
    [
        ("errata_version", "errata_id"),
        ("errata_version", "reason_unit_id"),
        ("errata_version", "source_id"),
        ("errata_version", "supersedes_id"),
        ("errata_change", "errata_version_id"),
        ("errata_change", "face_id"),
        ("errata_change", "before_revision_id"),
        ("errata_change", "after_revision_id"),
        ("errata_printing", "errata_version_id"),
        ("errata_printing", "printing_id"),
        ("errata_printing", "decision_id"),
        ("source_correction", "printing_id"),
        ("source_correction", "face_id"),
        ("source_correction", "expected_source_unit_id"),
        ("correction_evidence", "correction_id"),
        ("correction_evidence", "source_id"),
        ("correction_application", "correction_id"),
        ("correction_application", "source_id"),
        ("correction_application", "result_unit_id"),
        ("correction_application", "face_revision_id"),
        ("qa_version", "qa_id"),
        ("qa_version", "question_unit_id"),
        ("qa_version", "answer_unit_id"),
        ("qa_version", "source_id"),
        ("qa_version", "supersedes_id"),
        ("qa_card", "qa_version_id"),
        ("qa_card", "card_id"),
        ("card_related", "from_card_id"),
        ("card_related", "to_card_id"),
        ("card_related", "target_printing_id"),
        ("card_related", "source_id"),
        ("art", "card_id"),
        ("art", "face_id"),
        ("region_mapping_review", "card_id"),
        ("region_mapping_review", "source_id"),
        ("region_text_review", "card_id"),
        ("region_text_review", "decision_id"),
        ("region_divergence", "card_id"),
        ("region_divergence", "source_id"),
        ("region_divergence", "decision_id"),
    ],
)
def test_fixed_foreign_key_counterexamples(
    db: Database, table: str, column: str
) -> None:
    with pytest.raises(sqlite3.IntegrityError), db.transaction():
        db.update(table, key(table), {column: "missing"})


@pytest.mark.parametrize(
    "table",
    [
        "errata",
        "errata_version",
        "errata_change",
        "errata_printing",
        "source_correction",
        "correction_evidence",
        "correction_application",
        "qa",
        "qa_version",
        "qa_card",
        "card_related",
        "art",
        "region_mapping_review",
        "region_text_review",
        "region_divergence",
    ],
)
def test_each_primary_key_is_unique(db: Database, table: str) -> None:
    with pytest.raises(sqlite3.IntegrityError, match="UNIQUE"), db.transaction():
        db.insert(table, rows()[table])


def test_art_reference_is_null_only_when_group_disabled() -> None:
    with create_database(compile_minimum()) as database:
        with database.transaction():
            populate(database, include_en=False)
        with pytest.raises(sqlite3.IntegrityError), database.transaction():
            database.update(
                "printing_face",
                {"printing_id": "printing", "face_id": "face"},
                {"art_id": "art"},
            )


def test_rebuild_revalidates_art(tmp_path: Path) -> None:
    path = tmp_path / "build.sqlite"
    old = compile_schema(REGISTRY, ("images", "cr"), t0_schemas(), version=2)
    with create_database(old, path) as database:
        with database.transaction():
            populate_a(database)
    before = path.read_bytes()

    def invalid(database: Database) -> None:
        populate(database)
        database.update(
            "printing_face",
            {"printing_id": "printing", "face_id": "face"},
            {"art_id": "missing"},
        )

    with pytest.raises(sqlite3.IntegrityError, match="Foreign key"):
        rebuild_database(compile_minimum(include_en=True), path, invalid)
    assert path.read_bytes() == before
    rebuild_database(compile_minimum(include_en=True), path, populate)
    with sqlite3.connect(path) as connection:
        assert connection.execute("PRAGMA user_version").fetchall() == [(6,)]
        assert connection.execute(
            "SELECT count(*) FROM sqlite_schema WHERE type='table'"
        ).fetchall() == [(61,)]
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
    assert list(tmp_path.iterdir()) == [path]
