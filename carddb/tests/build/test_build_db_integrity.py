"""Saving skips CHECK revalidation but keeps SQLite's page and index corruption checks."""

import sqlite3
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database

from .test_build_db import compiled

if TYPE_CHECKING:
    from pathlib import Path


def _damaged_page(connection: sqlite3.Connection, table: str) -> bytes:
    page_size = connection.execute("PRAGMA page_size").fetchone()[0]
    root = connection.execute(
        "SELECT rootpage FROM sqlite_schema WHERE name = ?", (table,)
    ).fetchone()[0]
    assert isinstance(page_size, int)
    assert isinstance(root, int)
    content = bytearray(connection.serialize())
    # An invalid b-tree page type represents physical damage, independent of CHECK.
    content[(root - 1) * page_size] = 0
    return bytes(content)


def test_sqlite_index_corruption_is_reported_while_ignoring_check() -> None:
    with sqlite3.connect(":memory:", isolation_level=None) as connection:
        connection.executescript(
            "CREATE TABLE sample (value INTEGER);"
            "CREATE UNIQUE INDEX sample_value ON sample(value);"
            "INSERT INTO sample VALUES (1);"
            "CREATE TABLE empty (value INTEGER);"
            "CREATE UNIQUE INDEX empty_value ON empty(value);"
            "PRAGMA writable_schema = ON;"
            "UPDATE sqlite_schema SET rootpage = "
            "(SELECT rootpage FROM sqlite_schema WHERE name = 'empty_value') "
            "WHERE name = 'sample_value';"
        )
        content = connection.serialize()
    with sqlite3.connect(":memory:", isolation_level=None) as damaged:
        damaged.deserialize(content)
        damaged.execute("PRAGMA ignore_check_constraints = ON")
        errors = damaged.execute("PRAGMA integrity_check").fetchall()
    assert ("row 1 missing from index sample_value",) in errors


def test_save_restores_check_enforcement(tmp_path: Path) -> None:
    with create_database(compiled()) as db:
        with db.transaction():
            db.insert("node", {"id": "synthetic:a"})
        db.save(tmp_path / "build.sqlite")
        with pytest.raises(sqlite3.IntegrityError, match="CHECK constraint failed"):
            db._connection.execute("INSERT INTO node (id) VALUES ('')")
        with db.transaction():
            db.insert("node", {"id": "synthetic:b"})


def test_failed_save_restores_check_enforcement(tmp_path: Path) -> None:
    destination = tmp_path / "build.sqlite"
    with create_database(compiled()) as db:
        db._connection.deserialize(_damaged_page(db._connection, "node"))
        with pytest.raises(sqlite3.DatabaseError, match="malformed"):
            db.save(destination)
        assert db._connection.execute("PRAGMA ignore_check_constraints").fetchall() == [
            (0,)
        ]
    assert not destination.exists()


def test_save_rejects_disabled_check_enforcement(tmp_path: Path) -> None:
    destination = tmp_path / "build.sqlite"
    with create_database(compiled()) as db:
        db._connection.execute("PRAGMA ignore_check_constraints = ON")
        with pytest.raises(sqlite3.IntegrityError, match="enforcement is disabled"):
            db.save(destination)
    assert not destination.exists()
