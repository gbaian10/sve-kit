"""Exercise the T1 CR tables with independently failing synthetic constraints."""

import sqlite3
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import (
    Capability,
    Column,
    Kind,
    QueryCheck,
    Registry,
    Table,
    compile_schema,
    create_database,
)
from sve_carddb.build.t0 import TABLES as T0_TABLES
from sve_carddb.build.t0 import compile_t0
from sve_carddb.build.t1 import REGISTRY, compile_build
from sve_carddb.build.t1_cr import TABLES as CR_TABLES

from ..support.build_db_fixtures import seed
from ..support.build_db_t1_fixtures import populate, rows

if TYPE_CHECKING:
    from ..support.database_fixtures import DatabaseTemplate


TABLES = CR_TABLES


def test_exact_inventory_and_optional_closure() -> None:
    expected = {
        "cr_version",
        "cr_clause",
    }
    base = {table.name for table in T0_TABLES}
    assert {table.name for table in TABLES} == expected
    assert {table.name for table in compile_build(("cr",)).tables} == base | expected
    assert len(compile_build(("cr",)).tables) == 44
    assert {table.name for table in compile_build().tables} == base
    assert compile_build(("cr",)).sql == compile_build(("cr", "t0")).sql
    for cap in ("dsl", "unknown"):
        with pytest.raises(ValueError, match="Unknown or unimplemented"):
            compile_build((cap,))


def test_connected_graph_and_pragma_inventory() -> None:
    schema = compile_build(("cr",))
    with create_database(schema) as db:
        with db.transaction():
            populate(db)
        for table in schema.tables:
            assert db.rows(table.name)
        assert db._read("PRAGMA user_version") == ((15,),)
        assert db._read("SELECT count(*) FROM sqlite_schema WHERE type='table'") == (
            (44,),
        )
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


@pytest.mark.parametrize("requested", [("t0",), ("cr",)])
def test_ddl_does_not_claim_usable_preview_or_release(
    requested: tuple[str, ...],
) -> None:
    with pytest.raises(ValueError, match="Importer/validator unavailable"):
        REGISTRY.require_usable(requested)


class TestConnectedGraphConstraints:
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
