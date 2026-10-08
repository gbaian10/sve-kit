"""Bounded typed lookups preserve whitelists, NULL matching and exact inserts."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build.rows import insert_exact

if TYPE_CHECKING:
    from sve_carddb.build import Value

    from .database_fixtures import DatabaseTemplate


def test_filtered_read_matches_nullable_values_and_does_not_change_unfiltered_reads(
    t0_database_template: DatabaseTemplate,
) -> None:
    with t0_database_template.copy() as db:
        all_rows = db.select("source_record", ("id", "authored_path"))
        null_rows = db.select(
            "source_record", ("id", "authored_path"), where={"authored_path": None}
        )
        assert null_rows == all_rows
        assert db.select("source_record", ("id",), where={"id": "missing"}) == ()
        assert db.select("source_record", ("id",), where={}) == db.select(
            "source_record", ("id",)
        )


def test_filter_values_are_parameters_not_sql(
    t0_database_template: DatabaseTemplate,
) -> None:
    with t0_database_template.copy() as db:
        assert db.select("source_record", ("id",), where={"id": "' OR 1=1 --"}) == ()
        rows = db.select(
            "source_record", ("id",), where={"id": "source", "kind": "authored"}
        )
        assert len(rows) == 1
        assert rows[0].values == {"id": "source"}


def test_filter_columns_are_validated_before_sql(
    t0_database_template: DatabaseTemplate,
) -> None:
    with (
        t0_database_template.copy() as db,
        pytest.raises(ValueError, match=r"^Unknown column source_record\.missing$"),
    ):
        db.select("source_record", ("id",), where={"missing": "source"})


def test_filter_types_are_validated_before_sql(
    t0_database_template: DatabaseTemplate,
) -> None:
    with (
        t0_database_template.copy() as db,
        pytest.raises(TypeError, match=r"^Expected text for id$"),
    ):
        db.select("source_record", ("id",), where={"id": 1})


def test_exact_insert_remains_idempotent_and_conflicts_are_rejected(
    t0_database_template: DatabaseTemplate,
) -> None:
    with t0_database_template.copy() as db:
        with db.transaction():
            values = dict(db.rows("source_record")[0].values)
            insert_exact(db, "source_record", values, ("id",))
        with (
            pytest.raises(ValueError, match=r"^Conflicting catalog row$"),
            db.transaction(),
        ):
            insert_exact(
                db,
                "source_record",
                values | {"authored_path": "synthetic/new.yaml"},
                ("id",),
            )
        assert len(db.rows("source_record")) == 1


def test_exact_insert_compares_omitted_nullable_columns(
    t0_database_template: DatabaseTemplate,
) -> None:
    with t0_database_template.copy() as db:
        values = dict(db.rows("source_record")[0].values)
        sparse: dict[str, Value] = {
            name: value for name, value in values.items() if value is not None
        }
        with db.transaction():
            insert_exact(db, "source_record", sparse, ("id",))
        assert db.rows("source_record")[0].values == values
        with db.transaction():
            db.update(
                "source_record",
                {"id": values["id"]},
                {"authored_path": "synthetic/source.yaml"},
            )
        with (
            pytest.raises(ValueError, match=r"^Conflicting catalog row$"),
            db.transaction(),
        ):
            insert_exact(db, "source_record", sparse, ("id",))
