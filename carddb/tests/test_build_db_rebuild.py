"""Schema upgrades rebuild from input and preserve the old artifact on failure."""

import sqlite3
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database, rebuild_database
from sve_carddb.build_db.compiler import compile_schema
from sve_carddb.build_db.t0 import REGISTRY, compile_t0
from sve_carddb.build_db.t1 import compile_build

from .build_db_fixtures import seed
from .build_db_t1_fixtures import populate

if TYPE_CHECKING:
    from sve_carddb.build_db import Database


def test_rebuild_upgrades_offline_file_and_adds_real_cr_fk(tmp_path: Path) -> None:
    path = tmp_path / "build.sqlite"
    with create_database(compile_t0(), path) as db:
        seed(db)
    original = path.read_bytes()
    rebuild_database(compile_build(("images", "cr")), path, populate)
    assert path.read_bytes() != original
    with sqlite3.connect(path) as connection:
        assert connection.execute("PRAGMA user_version").fetchall() == [(2,)]
        assert connection.execute(
            "SELECT cr_version_id FROM rules_profile_revision"
        ).fetchall() == [("cr",)]
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
    assert list(tmp_path.iterdir()) == [path]


@pytest.mark.parametrize("failure", ["body", "fk", "query"])
def test_rebuild_failure_preserves_previous_bytes(tmp_path: Path, failure: str) -> None:
    path = tmp_path / "build.sqlite"
    with create_database(compile_t0(), path) as db:
        seed(db)
    original = path.read_bytes()

    def invalid(db: Database) -> None:
        populate(db)
        if failure == "body":
            raise RuntimeError("Synthetic importer failure")
        if failure == "fk":
            db.update(
                "rules_profile_revision",
                {"id": "profile_revision"},
                {"cr_version_id": "missing"},
            )
        else:
            db.update("image_asset", {"id": "image"}, {"publication_state": "pending"})

    with pytest.raises((RuntimeError, sqlite3.IntegrityError)):
        rebuild_database(compile_build(("images", "cr")), path, invalid)
    assert path.read_bytes() == original
    assert list(tmp_path.iterdir()) == [path]


def test_replacement_failure_cleans_candidate(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = tmp_path / "build.sqlite"
    with create_database(compile_t0(), path):
        pass
    original = path.read_bytes()

    def fail_replace(_source: Path, _target: Path) -> Path:
        raise OSError("Synthetic replacement failure")

    monkeypatch.setattr(Path, "replace", fail_replace)
    with pytest.raises(OSError, match="replacement"):
        rebuild_database(compile_build(("images", "cr")), path, populate)
    assert path.read_bytes() == original
    assert list(tmp_path.iterdir()) == [path]


@pytest.mark.parametrize("suffix", ["-wal", "-shm", "-journal"])
def test_rebuild_rejects_existing_sidecars(tmp_path: Path, suffix: str) -> None:
    path = tmp_path / "build.sqlite"
    sidecar = tmp_path / (path.name + suffix)
    sidecar.write_bytes(b"synthetic")
    with pytest.raises(ValueError, match="sidecars"):
        rebuild_database(compile_build(), path, lambda _db: None)
    assert sidecar.read_bytes() == b"synthetic"
    assert not path.exists()


def test_rebuild_rejects_symlink(tmp_path: Path) -> None:
    original = tmp_path / "original.sqlite"
    original.write_bytes(b"synthetic")
    link = tmp_path / "build.sqlite"
    link.symlink_to(original)
    with pytest.raises(ValueError, match="symlink"):
        rebuild_database(compile_build(), link, lambda _db: None)
    assert original.read_bytes() == b"synthetic"


def test_new_relative_destination(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.chdir(tmp_path)
    Path("dist").mkdir()
    rebuild_database(compile_build(), Path("dist/build.sqlite"), lambda _db: None)
    assert Path("dist/build.sqlite").is_file()
    assert len(list(Path("dist").iterdir())) == 1


@pytest.mark.parametrize("version", [0, -1, 2**31, True])
def test_invalid_schema_version(version: int) -> None:
    with pytest.raises(ValueError, match="version"):
        compile_schema(REGISTRY, ("t0",), version=version)


def test_schema_version_tampering_fails_verification() -> None:
    with create_database(compile_build()) as db:
        with (
            pytest.raises(sqlite3.IntegrityError, match="version mismatch"),
            db.transaction(),
        ):
            db._connection.execute("PRAGMA user_version = 99")
        assert db._read("PRAGMA user_version") == ((2,),)
