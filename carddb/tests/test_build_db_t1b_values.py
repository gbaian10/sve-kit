"""Field-dependent correction values share the public contract, including JSON null."""

import sqlite3
from typing import TYPE_CHECKING

import pytest
from jsonschema import ValidationError

from sve_carddb.build_db import Json

from .test_build_db_t1b import db as db  # ruff: ignore[useless-import-alias] -- explicitly re-export the pytest fixture
from .test_build_db_t1b import key

if TYPE_CHECKING:
    from sve_carddb.build_db import Database


@pytest.mark.parametrize(
    "columns",
    [
        ("errata_change", "before_value", "after_value"),
        ("source_correction", "expected_raw_value", "corrected_value"),
    ],
)
@pytest.mark.parametrize(
    ("field", "good", "bad"),
    [
        ("effect", Json(""), Json(1)),
        ("name", Json("Name"), Json(None)),
        ("card_type", Json("Raw type"), Json([])),
        ("flavor", Json("Text"), Json(0)),
        ("other", Json("Description"), Json([])),
        ("cost", Json(None), Json("3")),
        ("attack", Json(-1), Json([])),
        ("defense", Json(3), Json("")),
        ("traits", Json(["B", "A", "A"]), Json("Trait")),
        ("titles", Json([]), Json(None)),
        ("special_kinds", Json(["Token"]), Json(0)),
    ],
)
@pytest.mark.parametrize("side", ["before", "after"])
def test_field_value_mapping(
    db: Database,
    columns: tuple[str, str, str],
    field: str,
    good: Json,
    bad: Json,
    side: str,
) -> None:
    table, before, after = columns
    with db.transaction():
        db.update(table, key(table), {"field": field, before: good, after: good})
    with pytest.raises(sqlite3.IntegrityError), db.transaction():
        db.update(table, key(table), {(before if side == "before" else after): bad})
    assert db.rows(table)[0].values[before] == good
    assert db.rows(table)[0].values[after] == good


@pytest.mark.parametrize("table", ["errata_change", "source_correction"])
def test_unknown_field_is_not_an_escape_hatch(db: Database, table: str) -> None:
    with pytest.raises(sqlite3.IntegrityError), db.transaction():
        db.update(table, key(table), {"field": "not_known"})


@pytest.mark.parametrize(
    ("table", "column"),
    [
        ("errata_change", "before_value"),
        ("errata_change", "after_value"),
        ("source_correction", "expected_raw_value"),
        ("source_correction", "corrected_value"),
    ],
)
@pytest.mark.parametrize(
    "bad",
    [Json(True), Json(1.5), Json(2**53), Json({"text": "synthetic"}), Json([None])],
)
def test_json_columns_reject_non_contract_values(
    db: Database, table: str, column: str, bad: Json
) -> None:
    with pytest.raises((ValueError, TypeError, ValidationError)), db.transaction():
        db.update(table, key(table), {column: bad})
