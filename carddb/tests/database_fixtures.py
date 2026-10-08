"""Private connections restored from an immutable, genuinely populated database."""

import sqlite3
from contextlib import contextmanager
from dataclasses import dataclass
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Database, create_database
from sve_carddb.build.database import install_functions
from sve_carddb.build.t0 import compile_t0
from sve_carddb.build.t1 import compile_build, compile_minimum

from .build_db_fixtures import seed
from .build_db_t1_fixtures import populate as populate_t1
from .build_db_t1b_fixtures import populate as populate_t1b

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build import CompiledSchema


@dataclass(frozen=True)
class DatabaseTemplate:
    schema: CompiledSchema
    content: bytes

    @contextmanager
    def copy(self) -> Iterator[Database]:
        # The verified graph is shared as bytes; connection state and Rules never are.
        connection = sqlite3.connect(":memory:", isolation_level=None)
        try:
            connection.deserialize(self.content)
            rules = install_functions(connection, self.schema)
            connection.execute("PRAGMA foreign_keys = ON")
            yield Database(connection, self.schema, rules)
        finally:
            connection.close()


@pytest.fixture(scope="session")
def t0_database_template() -> DatabaseTemplate:
    schema = compile_t0()
    with create_database(schema) as database:
        seed(database)
        return DatabaseTemplate(schema, database._connection.serialize())


@pytest.fixture(scope="session")
def t1b_database_template() -> DatabaseTemplate:
    schema = compile_minimum(include_en=True)
    with create_database(schema) as database:
        with database.transaction():
            populate_t1b(database)
        return DatabaseTemplate(schema, database._connection.serialize())


@pytest.fixture(scope="session")
def t1_database_template() -> DatabaseTemplate:
    schema = compile_build(("images", "cr"))
    with create_database(schema) as database:
        with database.transaction():
            populate_t1(database)
        return DatabaseTemplate(schema, database._connection.serialize())


@pytest.fixture(scope="session")
def image_parent_database_template() -> DatabaseTemplate:
    schema = compile_build(("images",))
    with create_database(schema) as database:
        seed(database)
        return DatabaseTemplate(schema, database._connection.serialize())
