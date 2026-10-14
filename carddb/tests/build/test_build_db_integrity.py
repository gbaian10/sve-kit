"""CHECK skipping preserves SQLite's page and index corruption detection."""

import sqlite3
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database

from .test_build_db import compiled

if TYPE_CHECKING:
    from pathlib import Path


def test_sqlite_check_skipping() -> None:
    with sqlite3.connect(":memory:", isolation_level=None) as connection:
        connection.execute("CREATE TABLE sample (value INTEGER CHECK (value > 0))")
        connection.execute("PRAGMA ignore_check_constraints = ON")
        connection.execute("INSERT INTO sample VALUES (-1)")
        assert connection.execute("PRAGMA integrity_check").fetchall() == [("ok",)]
        connection.execute("PRAGMA ignore_check_constraints = OFF")
        assert connection.execute("PRAGMA integrity_check").fetchall() == [
            ("CHECK constraint failed in sample",)
        ]


def test_sqlite_page_corruption() -> None:
    with sqlite3.connect(":memory:", isolation_level=None) as connection:
        connection.execute("CREATE TABLE sample (value INTEGER)")
        connection.execute("INSERT INTO sample VALUES (1)")
        page_size = connection.execute("PRAGMA page_size").fetchone()[0]
        root = connection.execute(
            "SELECT rootpage FROM sqlite_schema WHERE name = 'sample'"
        ).fetchone()[0]
        assert isinstance(page_size, int)
        assert isinstance(root, int)
        content = bytearray(connection.serialize())
        # An invalid b-tree page type represents physical damage, independent of CHECK.
        content[(root - 1) * page_size] = 0
    for flag in ("ON", "OFF"):
        with sqlite3.connect(":memory:", isolation_level=None) as damaged:
            damaged.deserialize(bytes(content))
            damaged.execute(f"PRAGMA ignore_check_constraints = {flag}")
            with pytest.raises(sqlite3.DatabaseError, match="malformed"):
                damaged.execute("PRAGMA integrity_check").fetchall()


def test_sqlite_index_corruption() -> None:
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
    for flag in ("ON", "OFF"):
        with sqlite3.connect(":memory:", isolation_level=None) as damaged:
            damaged.deserialize(content)
            damaged.execute(f"PRAGMA ignore_check_constraints = {flag}")
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
