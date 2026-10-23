"""Private connections restored from an immutable, genuinely populated database."""

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t0 import compile_t0
from sve_carddb.build.t1 import compile_build, compile_minimum

from .build_db_fixtures import seed
from .build_db_t1_fixtures import populate as populate_t1
from .build_db_t1b_fixtures import populate as populate_t1b
from .database_template import DatabaseTemplate


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
    schema = compile_build(("cr",))
    with create_database(schema) as database:
        with database.transaction():
            populate_t1(database)
        return DatabaseTemplate(schema, database._connection.serialize())
