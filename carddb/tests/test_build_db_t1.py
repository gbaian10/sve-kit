"""Exercise the six T1 tables with independently failing synthetic constraints."""

import sqlite3
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import (
    Capability,
    Column,
    Kind,
    QueryCheck,
    Registry,
    Table,
    compile_schema,
    create_database,
)
from sve_carddb.build_db.t0 import TABLES as T0_TABLES
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.build_db.t1 import REGISTRY, compile_build
from sve_carddb.build_db.t1_cr import TABLES as CR_TABLES
from sve_carddb.build_db.t1_images import TABLES as IMAGE_TABLES

from .build_db_fixtures import seed
from .build_db_t1_fixtures import populate, rows

if TYPE_CHECKING:
    from sve_carddb.build_db import Value

    from .database_fixtures import DatabaseTemplate


TABLES = (*CR_TABLES, *IMAGE_TABLES)


def test_exact_inventory_and_optional_closure() -> None:
    expected = {
        "image_asset",
        "printing_image",
        "image_variant",
        "image_size",
        "cr_version",
        "cr_clause",
    }
    base = {table.name for table in T0_TABLES}
    assert {table.name for table in TABLES} == expected
    assert {
        table.name for table in compile_build(("images", "cr")).tables
    } == base | expected
    assert len(compile_build(("images", "cr")).tables) == 46
    assert len(compile_build(("images",)).tables) == 44
    assert len(compile_build(("cr",)).tables) == 42
    assert {table.name for table in compile_build().tables} == base
    assert (
        compile_build(("images", "cr")).sql == compile_build(("cr", "images", "t0")).sql
    )
    for cap in ("dsl", "keyword", "unknown"):
        with pytest.raises(ValueError, match="Unknown or unimplemented"):
            compile_build((cap,))


def test_connected_graph_and_pragma_inventory() -> None:
    schema = compile_build(("images", "cr"))
    with create_database(schema) as db:
        with db.transaction():
            populate(db)
        for table in schema.tables:
            assert db.rows(table.name)
        assert db._read("PRAGMA user_version") == ((4,),)
        assert db._read("SELECT count(*) FROM sqlite_schema WHERE type='table'") == (
            (46,),
        )
        assert db.rows("image_asset")[0].values["source_src_raw"] == "../source.png"
        assert db.rows("cr_clause")[0].values["number"] == "1.10.2"
        db.verify()


def test_old_nullable_cr_reference_is_null_only_until_enabled() -> None:
    for schema in (compile_t0(), compile_build()):
        with create_database(schema) as db:
            seed(db)
            with pytest.raises(sqlite3.IntegrityError), db.transaction():
                db.update(
                    "rules_profile_revision",
                    {"id": "profile_revision"},
                    {"cr_version_id": "cr"},
                )
    with create_database(compile_build(("cr",))) as db:
        seed(db)
        with pytest.raises(sqlite3.IntegrityError), db.transaction():
            db.update(
                "rules_profile_revision",
                {"id": "profile_revision"},
                {"cr_version_id": "missing"},
            )
        with db.transaction():
            db.update(
                "rules_profile_revision",
                {"id": "profile_revision"},
                {"cr_version_id": "cr"},
            )
            db.insert("cr_version", rows()["cr_version"])


def test_query_dependencies_close_and_require_pipeline_readiness() -> None:
    alpha = Table(
        "alpha",
        (Column("id", Kind.ID),),
        ("id",),
        query_checks=(QueryCheck("no_beta", "SELECT 1 FROM beta LIMIT 1", ("beta",)),),
    )
    beta = Table("beta", (Column("id", Kind.ID),), ("id",))
    registry = Registry(
        (alpha, beta),
        (
            Capability("alpha", ("alpha",), importer_ready=True, validator_ready=True),
            Capability("beta", ("beta",)),
        ),
    )
    assert {table.name for table in registry.resolve(("alpha",))} == {"alpha", "beta"}
    with pytest.raises(ValueError, match="Importer/validator unavailable: beta"):
        registry.require_usable(("alpha",))
    with create_database(compile_schema(registry, ("alpha",))) as db:
        with pytest.raises(sqlite3.IntegrityError, match="no_beta"), db.transaction():
            db.insert("beta", {"id": "x"})
        assert not db.rows("beta")
    ready = replace(registry.capabilities[1], importer_ready=True, validator_ready=True)
    replace(registry, capabilities=(registry.capabilities[0], ready)).require_usable(
        ("alpha",)
    )
    for importer, validator in ((False, False), (True, False), (False, True)):
        with pytest.raises(ValueError, match="Importer/validator"):
            replace(
                registry,
                capabilities=(
                    registry.capabilities[0],
                    replace(ready, importer_ready=importer, validator_ready=validator),
                ),
            ).require_usable(("alpha",))
    reserved = replace(
        registry,
        tables=(alpha,),
        capabilities=(registry.capabilities[0], replace(ready, implemented=False)),
    )
    with pytest.raises(ValueError, match="Unknown or unimplemented"):
        reserved.resolve(("alpha",))
    with pytest.raises(ValueError, match="Unregistered query"):
        replace(
            registry, tables=(alpha,), capabilities=(registry.capabilities[0],)
        ).validate()


@pytest.mark.parametrize("requested", [("t0",), ("images",), ("cr",), ("images", "cr")])
def test_ddl_does_not_claim_usable_preview_or_release(
    requested: tuple[str, ...],
) -> None:
    with pytest.raises(ValueError, match="Importer/validator unavailable"):
        REGISTRY.require_usable(requested)


class TestImageParentConstraints:
    @pytest.mark.parametrize(
        ("field", "value"),
        [
            ("withdrawal_reason", None),
            ("withdrawal_reason", ""),
            ("withdrawal_reason", "   "),
        ],
    )
    def test_withdrawn_requires_reason(
        self, image_parent_database_template: DatabaseTemplate, field: str, value: Value
    ) -> None:
        with image_parent_database_template.copy() as db:
            with pytest.raises(sqlite3.IntegrityError), db.transaction():
                db.insert(
                    "image_asset",
                    rows()["image_asset"]
                    | {"publication_state": "withdrawn", field: value},
                )

    @pytest.mark.parametrize(
        ("field", "value"),
        [
            ("content_hash", None),
            ("mime", None),
            ("mime", ""),
            ("width", None),
            ("width", 0),
            ("height", None),
            ("height", 0),
            ("bytes", None),
        ],
    )
    def test_available_requires_metadata(
        self, image_parent_database_template: DatabaseTemplate, field: str, value: Value
    ) -> None:
        with image_parent_database_template.copy() as db:
            with pytest.raises(sqlite3.IntegrityError), db.transaction():
                db.insert(
                    "image_asset",
                    rows()["image_asset"]
                    | {"publication_state": "pending", field: value},
                )

    @pytest.mark.parametrize("availability", ["missing", "unfetched"])
    def test_unavailable_metadata_can_be_unknown(
        self, image_parent_database_template: DatabaseTemplate, availability: str
    ) -> None:
        with image_parent_database_template.copy() as db:
            with db.transaction():
                db.insert(
                    "image_asset",
                    rows()["image_asset"]
                    | {
                        "availability": availability,
                        "publication_state": "pending",
                        "review_decision_id": None,
                        "content_hash": None,
                        "mime": None,
                        "width": None,
                        "height": None,
                        "bytes": None,
                    },
                )
            assert not db.rows("image_variant")

    @pytest.mark.parametrize(
        ("origin", "decision", "availability"),
        [
            ("third_party", None, "available"),
            ("official", "decision", "available"),
            ("official", None, "unfetched"),
            ("official", None, "missing"),
        ],
    )
    def test_approved_local_policy(
        self,
        image_parent_database_template: DatabaseTemplate,
        origin: str,
        decision: str | None,
        availability: str,
    ) -> None:
        with image_parent_database_template.copy() as db:
            with pytest.raises(sqlite3.IntegrityError), db.transaction():
                db.insert(
                    "image_asset",
                    rows()["image_asset"]
                    | {
                        "origin": origin,
                        "review_decision_id": decision,
                        "availability": availability,
                    },
                )

    def test_official_available_without_variants_needs_no_human_decision(
        self, image_parent_database_template: DatabaseTemplate
    ) -> None:
        with image_parent_database_template.copy() as db:
            with db.transaction():
                db.insert(
                    "image_asset",
                    rows()["image_asset"]
                    | {"origin": "official", "review_decision_id": None},
                )
            assert not db.rows("image_variant")


class TestConnectedGraphConstraints:
    @pytest.mark.parametrize(
        ("field", "value"),
        [
            ("width", 0),
            ("height", 0),
            ("format", "png"),
            ("path", "images/sha256/bb/" + "a" * 64 + ".webp"),
            ("path", "images/sha256/aa/" + "b" * 64 + ".webp"),
            ("path", "private/source.png"),
        ],
    )
    def test_variant_constraints(
        self, t1_database_template: DatabaseTemplate, field: str, value: Value
    ) -> None:
        with t1_database_template.copy() as db:
            with pytest.raises(sqlite3.IntegrityError), db.transaction():
                db.update(
                    "image_variant",
                    {"image_id": "image", "size_key": "card_s", "format": "webp"},
                    {field: value},
                )

    @pytest.mark.parametrize(
        ("field", "value"),
        [
            ("publication_state", "pending"),
            ("publication_state", "withdrawn"),
            ("availability", "missing"),
            ("availability", "unfetched"),
        ],
    )
    def test_variant_rechecks_parent_updates_and_rolls_back(
        self, t1_database_template: DatabaseTemplate, field: str, value: Value
    ) -> None:
        with t1_database_template.copy() as db:
            before = db.rows("image_asset")
            with (
                pytest.raises(
                    sqlite3.IntegrityError, match="image_variant_publishable"
                ),
                db.transaction(),
            ):
                db.update(
                    "image_asset",
                    {"id": "image"},
                    {field: value, "withdrawal_reason": "Synthetic withdrawal"},
                )
            assert db.rows("image_asset") == before
            assert db.rows("image_variant")
            with db.transaction():
                db.delete(
                    "image_variant",
                    {"image_id": "image", "size_key": "card_s", "format": "webp"},
                )
                db.update(
                    "image_asset",
                    {"id": "image"},
                    {field: value, "withdrawal_reason": "Synthetic withdrawal"},
                )

    @pytest.mark.parametrize(
        "state", ["sampled", "model_reviewed", "proposed", "rejected", "disputed"]
    )
    def test_review_rechecked_after_parent_update(
        self, t1_database_template: DatabaseTemplate, state: str
    ) -> None:
        with t1_database_template.copy() as db:
            with (
                pytest.raises(sqlite3.IntegrityError, match="image_confirmed_review"),
                db.transaction(),
            ):
                db.update("decision", {"id": "decision"}, {"state": state})
            assert db.rows("decision")[0].values["state"] == "confirmed"

    def test_review_source_deletion_fails_and_rolls_back(
        self, t1_database_template: DatabaseTemplate
    ) -> None:
        with t1_database_template.copy() as db:
            with (
                pytest.raises(sqlite3.IntegrityError, match="image_review_source"),
                db.transaction(),
            ):
                db.delete(
                    "decision_source",
                    {
                        "decision_id": "decision",
                        "source_id": "source",
                        "role": "synthetic",
                    },
                )
            assert db.rows("decision_source")

    def test_review_evidence_for_another_source_fails_and_rolls_back(
        self, t1_database_template: DatabaseTemplate
    ) -> None:
        with t1_database_template.copy() as db:
            with db.transaction():
                db.insert(
                    "source_record",
                    dict(db.rows("source_record")[0].values)
                    | {"id": "other_source", "sha256": "sha256:" + "b" * 64},
                )
            before = db.rows("decision_source")
            with (
                pytest.raises(sqlite3.IntegrityError, match="image_review_source"),
                db.transaction(),
            ):
                db.update(
                    "decision_source",
                    {
                        "decision_id": "decision",
                        "source_id": "source",
                        "role": "synthetic",
                    },
                    {"source_id": "other_source"},
                )
            assert db.rows("decision_source") == before

    def test_variant_requires_an_existing_image_size(
        self, t1_database_template: DatabaseTemplate
    ) -> None:
        with t1_database_template.copy() as db:
            before = db.rows("image_variant")
            with (
                pytest.raises(sqlite3.IntegrityError, match="Foreign key check failed"),
                db.transaction(),
            ):
                db.insert(
                    "image_variant",
                    rows()["image_variant"] | {"size_key": "missing_size"},
                )
            assert db.rows("image_variant") == before

    def test_original_size_parent_update_fails(
        self, t1_database_template: DatabaseTemplate
    ) -> None:
        with t1_database_template.copy() as db:
            with (
                pytest.raises(
                    sqlite3.IntegrityError, match="image_variant_not_original"
                ),
                db.transaction(),
            ):
                db.update("image_size", {"key": "card_s"}, {"is_original": True})
            assert db.rows("image_size")[0].values["is_original"] is False

    @pytest.mark.parametrize("table", TABLES, ids=lambda table: table.name)
    def test_every_primary_key_rejects_duplicates(
        self, t1_database_template: DatabaseTemplate, table: Table
    ) -> None:
        with t1_database_template.copy() as db:
            with pytest.raises(sqlite3.IntegrityError), db.transaction():
                db.insert(table.name, rows()[table.name])

    @pytest.mark.parametrize(
        ("table", "column"),
        [(table, fk.columns[0]) for table in TABLES for fk in table.foreign_keys],
    )
    def test_each_foreign_key_rejects_missing_target(
        self, t1_database_template: DatabaseTemplate, table: Table, column: str
    ) -> None:
        with t1_database_template.copy() as db:
            key = {name: rows()[table.name][name] for name in table.primary_key}
            with pytest.raises(sqlite3.IntegrityError), db.transaction():
                db.update(table.name, key, {column: "missing"})

    def test_printing_image_cannot_use_another_printings_face(
        self, t1_database_template: DatabaseTemplate
    ) -> None:
        with t1_database_template.copy() as db:
            with db.transaction():
                db.insert(
                    "face",
                    {
                        "id": "other_face",
                        "card_id": "old_card",
                        "ordinal": 0,
                        "side": "front",
                    },
                )
            with pytest.raises(sqlite3.IntegrityError), db.transaction():
                db.update(
                    "printing_image",
                    {"printing_id": "printing", "face_id": "face"},
                    {"face_id": "other_face"},
                )

    def test_cr_versions_preserve_same_label_changed_source(
        self, t1_database_template: DatabaseTemplate
    ) -> None:
        with t1_database_template.copy() as db:
            with pytest.raises(sqlite3.IntegrityError), db.transaction():
                db.insert("cr_version", rows()["cr_version"] | {"id": "duplicate"})
            with db.transaction():
                db.insert(
                    "source_record",
                    dict(db.rows("source_record")[0].values) | {"id": "source2"},
                )
                db.insert(
                    "cr_version",
                    rows()["cr_version"] | {"id": "cr2", "source_id": "source2"},
                )
                db.insert(
                    "cr_clause",
                    rows()["cr_clause"] | {"id": "clause2", "cr_version_id": "cr2"},
                )
            assert len(db.rows("cr_version")) == 2
            with pytest.raises(sqlite3.IntegrityError), db.transaction():
                db.insert("cr_clause", rows()["cr_clause"] | {"id": "duplicate"})
