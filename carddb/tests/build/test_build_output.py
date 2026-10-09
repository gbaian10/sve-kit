"""Failed build publication cannot overwrite output or leave partial directories."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build import output as build_output
from sve_carddb.build.output import save

from .test_build_db import compiled

if TYPE_CHECKING:
    from pathlib import Path


def test_existing_destination_is_refused(tmp_path: Path) -> None:
    destination = tmp_path / "saved"
    destination.mkdir()
    marker = destination / "existing"
    marker.write_text("preserved")
    with create_database(compiled()) as db, pytest.raises(FileExistsError):
        save(db, destination, b"{}", {})
    assert marker.read_text() == "preserved"
    assert list(tmp_path.iterdir()) == [destination]


def test_database_save_requires_completed_transaction(tmp_path: Path) -> None:
    destination = tmp_path / "build.sqlite"
    with create_database(compiled()) as db, db.transaction():
        with pytest.raises(RuntimeError, match="unfinished build transaction"):
            db.save(destination)
    assert not destination.exists()


def test_failed_rename_cleans_temporary_output(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    destination = tmp_path / "saved"

    def fail_rename(source: Path, target: Path) -> None:
        assert {p.name for p in source.iterdir()} == {
            "build.sqlite",
            "inputs.json",
            "report.json",
        }
        assert target == destination
        raise OSError("Synthetic rename failure")

    monkeypatch.setattr(build_output, "_rename_no_replace", fail_rename)
    with create_database(compiled()) as db:
        with pytest.raises(OSError, match="Synthetic rename failure"):
            save(db, destination, b"{}", {})
    assert not destination.exists()
    assert not list(tmp_path.iterdir())
