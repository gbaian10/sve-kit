"""Isolate defensive guards, including inputs already rejected by the SQL boundary."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import Row, create_database
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.routes import build_index, derive_routes, populate_routes
from sve_carddb.routes.defaults import _inclusion_date, select_defaults

from .build_db_fixtures import rows
from .routes_fixtures import base, printing

if TYPE_CHECKING:
    from sve_carddb.build_db import Database, Value


def replace_table(
    db: Database,
    monkeypatch: pytest.MonkeyPatch,
    table: str,
    replacement: tuple[Row, ...],
) -> None:
    original = db.rows

    def broken_rows(name: str) -> tuple[Row, ...]:
        return replacement if name == table else original(name)

    # These tests deliberately violate the upstream validated-row contract.
    monkeypatch.setattr(db, "rows", broken_rows)


def test_reserved_spellings_do_not_enter_folded_lookup() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            printing(db, "u", "UNIMPLEMENTED")
            printing(db, "p", "_PROVISIONAL")
            populate_routes(db)
        index = build_index(db)
        assert "unimplemented" not in index.folded
        assert "_provisional" not in index.folded
        assert index.resolve("/cards/UNIMPLEMENTED").printing_id == "u"
        assert index.resolve("/cards/Unimplemented").status == "missing"
        assert index.resolve("/cards/unimplemented").status == "reserved"


@pytest.mark.parametrize("old_key", ["old/key", "old\x00key", ""])
def test_official_alias_old_key_must_be_a_valid_path_segment(old_key: str) -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            printing(db, "new", "NEW")
            populate_routes(db)
            db.insert(
                "card_route_alias",
                values["card_route_alias"]
                | {"namespace": "official", "old_key": old_key, "target_key": "NEW"},
            )
        with pytest.raises(ValueError, match="Invalid card route key"):
            build_index(db)


@pytest.mark.parametrize("allocated", [0, -1, 2**32])
def test_defensive_uint32_guard_when_upstream_row_validation_is_broken(
    monkeypatch: pytest.MonkeyPatch,
    allocated: int,
) -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            printing(db, "a", "A")
        original = db.rows("card_int_id")[0]
        replace_table(
            db,
            monkeypatch,
            "card_int_id",
            (Row(original.table, dict(original.values) | {"int_id": allocated}),),
        )
        with pytest.raises(ValueError, match="permanent UInt32 allocation"):
            derive_routes(db)


def test_defensive_orphan_default_override_when_upstream_fk_validation_is_broken(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        replace_table(
            db,
            monkeypatch,
            "default_printing_override",
            (Row("default_printing_override", values["default_printing_override"]),),
        )
        with pytest.raises(ValueError, match="no card/region printing"):
            select_defaults(db)


@pytest.mark.parametrize("precision", ["month", "year", "unknown"])
def test_defensive_non_day_precision_wins_over_present_date(precision: str) -> None:
    values = rows()
    inclusion = Row(
        "printing_product",
        values["printing_product"]
        | {"first_available_on": "2019-01-01", "first_available_precision": precision},
    )
    product = Row(
        "product",
        values["product"] | {"released_on": "2010-01-01", "date_precision": "day"},
    )
    assert _inclusion_date(inclusion, product) is None


@pytest.mark.parametrize("invalid_date", ["2019-02-30", "2019-13-01", "not-a-date"])
@pytest.mark.parametrize("inherited", [False, True])
def test_defensive_effective_date_requires_a_valid_calendar_day(
    invalid_date: str, inherited: bool
) -> None:
    values = rows()
    inclusion = Row(
        "printing_product",
        values["printing_product"]
        | {
            "first_available_on": None if inherited else invalid_date,
            "first_available_precision": None if inherited else "day",
        },
    )
    product = Row(
        "product",
        values["product"] | {"released_on": invalid_date, "date_precision": "day"},
    )
    with pytest.raises(ValueError, match=r"day|month|Invalid isoformat"):
        _inclusion_date(inclusion, product)


def test_defensive_effective_date_rejects_non_text_storage() -> None:
    values = rows()
    inclusion: dict[str, Value] = values["printing_product"] | {
        "first_available_on": 2019,
        "first_available_precision": "day",
    }
    with pytest.raises(TypeError, match="Expected inclusion date"):
        _inclusion_date(
            Row("printing_product", inclusion), Row("product", values["product"])
        )
