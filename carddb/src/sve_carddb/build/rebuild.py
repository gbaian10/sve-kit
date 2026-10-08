"""Rebuild disposable databases from pinned inputs, never migrate rows in place."""

from pathlib import Path
from tempfile import TemporaryDirectory
from typing import TYPE_CHECKING

from sve_carddb.build.database import create_database

if TYPE_CHECKING:
    from collections.abc import Callable

    from sve_carddb.build.compiler import CompiledSchema
    from sve_carddb.build.database import Database


def rebuild_database(
    schema: CompiledSchema, destination: Path, populate: Callable[[Database], None]
) -> None:
    """Validate a fresh sibling file before atomic replacement of an offline build."""
    if destination.is_symlink():
        raise ValueError("Build destination must not be a symlink")
    if any(
        destination.with_name(destination.name + suffix).exists()
        for suffix in ("-wal", "-shm", "-journal")
    ):
        raise ValueError("Build destination has SQLite sidecars; close it first")
    with TemporaryDirectory(prefix=".build-db-", dir=destination.parent) as temporary:
        candidate = Path(temporary) / "build.sqlite"
        with create_database(schema, candidate) as database:
            with database.transaction():
                populate(database)
        candidate.replace(destination)
