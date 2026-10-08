"""Save a completed build database and report through a temporary directory."""

import os
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical
from sve_carddb.source_archive import _fsync_dir, _rename_no_replace

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build_db import Database


def save(db: Database, destination: Path, inputs: bytes, report: JsonValue) -> None:
    """Publish only after the database transaction and output writes have completed."""
    if destination.exists() or destination.is_symlink():
        raise FileExistsError("Build destination already exists")
    with TemporaryDirectory(prefix=".build-", dir=destination.parent) as temporary:
        candidate = Path(temporary) / "build"
        candidate.mkdir()
        db.save(candidate / "build.sqlite")
        (candidate / "inputs.json").write_bytes(inputs)
        (candidate / "report.json").write_bytes(canonical(report))
        for path in candidate.iterdir():
            with path.open("rb") as file:
                os.fsync(file.fileno())
        _fsync_dir(candidate)
        _rename_no_replace(candidate, destination)
        _fsync_dir(destination.parent)
