"""Reprint printings come only from the registry, never from card numbers."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.domains.registry.reprints import FILE, load_reprints, populate_reprints
from tests.support.schema_helpers import compile_t0

from ...support.build_db_fixtures import seed

if TYPE_CHECKING:
    from pathlib import Path

ROW = "- printing_id: printing\n  region: jp\n  card_no: TEST-001Ⓢa\n  basis: series_numbering\n"


def authored(tmp_path: Path, records: str) -> Path:
    path = tmp_path / FILE
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("format: 1\nkind: printing_reprints\nrecords:\n" + records)
    return tmp_path


def test_registered_printing_is_the_only_reprint(tmp_path: Path) -> None:
    records = load_reprints(authored(tmp_path, ROW))
    with create_database(compile_t0()) as db:
        seed(db)
        assert db.rows("printing")[0].values["reprint"] is None
        with db.transaction():
            populate_reprints(db, records)
        assert db.rows("printing")[0].values["reprint"] is True
    with pytest.raises(ValueError, match="Duplicate"):
        load_reprints(authored(tmp_path, ROW + ROW))


def test_unregistered_r_number_is_not_a_reprint() -> None:
    with create_database(compile_t0()) as db:
        seed(db)
        with db.transaction():
            db.update("printing", {"id": "printing"}, {"card_no": "TEST-R01"})
            populate_reprints(db, ())
        assert db.rows("printing")[0].values["reprint"] is None


def test_rejects_unknown_basis_and_identity_mismatch(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="Invalid reprint"):
        load_reprints(authored(tmp_path, ROW.replace("series_numbering", "guess")))
    records = load_reprints(authored(tmp_path, ROW.replace("TEST-001Ⓢa", "TEST-002")))
    with create_database(compile_t0()) as db:
        seed(db)
        with db.transaction(), pytest.raises(ValueError, match="identity mismatch"):
            populate_reprints(db, records)


def test_rejects_unknown_printing(tmp_path: Path) -> None:
    records = load_reprints(
        authored(tmp_path, ROW.replace("printing\n", "missing\n", 1))
    )
    with (
        create_database(compile_t0()) as db,
        pytest.raises(ValueError, match="Unknown reprint"),
    ):
        populate_reprints(db, records)
