"""Restore independent connections without importing a Python test runner."""

import sqlite3
from contextlib import contextmanager
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.build import Database
from sve_carddb.build.database import install_functions

if TYPE_CHECKING:
    import mmap
    from collections.abc import Generator

    from sve_carddb.build import CompiledSchema


@dataclass(frozen=True)
class DatabaseTemplate:
    schema: CompiledSchema
    content: bytes | mmap.mmap

    @contextmanager
    def copy(self) -> Generator[Database]:
        # The verified buffer is read-only; connection state and Rules never are shared.
        connection = sqlite3.connect(":memory:", isolation_level=None)
        try:
            connection.deserialize(self.content)
            rules = install_functions(connection, self.schema)
            connection.execute("PRAGMA foreign_keys = ON")
            yield Database(connection, self.schema, rules)
        finally:
            connection.close()
