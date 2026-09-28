"""Own SQLite connections and keep unchecked rows inside this boundary module."""

import sqlite3

# ruff: file-ignore[hardcoded-sql-expression] -- identifiers are whitelisted; all values are bound
from contextlib import contextmanager
from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING

from sve_carddb.build_db.model import Table, Value, identifier
from sve_carddb.build_db.t0_json import sorted_unique, symbol_valid
from sve_carddb.build_db.validation import Rules, SQLValue, decode, encode, fullmatch

if TYPE_CHECKING:
    from collections.abc import Iterator, Mapping
    from pathlib import Path

    from sve_carddb.build_db.compiler import CompiledSchema


@dataclass(frozen=True)
class Row:
    table: str
    values: Mapping[str, Value]


def _raw_rows(value: object) -> tuple[tuple[SQLValue, ...], ...]:
    if not isinstance(value, list):
        raise TypeError("SQLite did not return rows")
    result: list[tuple[SQLValue, ...]] = []
    for row in value:
        if not isinstance(row, tuple):
            raise TypeError("SQLite row must be a tuple")
        cells: list[SQLValue] = []
        for cell in row:
            if cell is None or isinstance(cell, str) or type(cell) is int:
                cells.append(cell)
            else:
                raise TypeError("Unexpected SQLite storage class")
        result.append(tuple(cells))
    return tuple(result)


def install_functions(connection: sqlite3.Connection, schema: CompiledSchema) -> Rules:
    """Install deterministic CHECK functions before executing compiled DDL."""
    rules = Rules(dict(schema.json_schemas))
    connection.create_function("sve_fullmatch", 2, fullmatch, deterministic=True)
    connection.create_function("sve_json_valid", 2, rules.sql_json, deterministic=True)
    connection.create_function(
        "sve_sorted_unique", 1, sorted_unique, deterministic=True
    )
    connection.create_function("sve_symbol_valid", 2, symbol_valid, deterministic=True)
    return rules


class Database:
    """Provide typed writes and reads; all mutations require an explicit transaction."""

    def __init__(
        self, connection: sqlite3.Connection, schema: CompiledSchema, rules: Rules
    ) -> None:
        self._connection = connection
        self._tables = {table.name: table for table in schema.tables}
        self._rules = rules

    def _read(self, sql: str) -> tuple[tuple[SQLValue, ...], ...]:
        result: object = self._connection.execute(sql).fetchall()
        return _raw_rows(result)

    def verify(self) -> None:
        """Fail on disabled foreign keys, dangling references or integrity errors."""
        if self._read("PRAGMA foreign_keys") != ((1,),):
            raise sqlite3.IntegrityError("Foreign key enforcement is disabled")
        if self._read("PRAGMA ignore_check_constraints") != ((0,),):
            raise sqlite3.IntegrityError("CHECK constraint enforcement is disabled")
        if self._read("PRAGMA foreign_key_check"):
            raise sqlite3.IntegrityError("Foreign key check failed")
        if self._read("PRAGMA integrity_check") != (("ok",),):
            raise sqlite3.IntegrityError("Integrity check failed")

    @contextmanager
    def transaction(self) -> Iterator[Database]:
        """Validate the complete graph before commit and roll back on any failure."""
        if self._connection.in_transaction:
            raise RuntimeError("Nested transactions are not supported")
        self._connection.execute("BEGIN")
        try:
            yield self
            self.verify()
            self._connection.execute("COMMIT")
        except BaseException:
            if self._connection.in_transaction:
                self._connection.execute("ROLLBACK")
            raise

    def _writable(self, table: str) -> Table:
        if not self._connection.in_transaction:
            raise RuntimeError("Writes require an explicit transaction")
        return self._tables[table]

    def _parameters(
        self, table: Table, values: Mapping[str, Value]
    ) -> tuple[SQLValue, ...]:
        return tuple(
            encode(table.column(name), value, self._rules)
            for name, value in values.items()
        )

    def insert(self, name: str, values: Mapping[str, Value]) -> None:
        """Bind validated values; omitted fixed columns use their declared defaults."""
        table = self._writable(name)
        parameters = self._parameters(table, values)
        if not values:
            self._connection.execute(f"INSERT INTO {identifier(name)} DEFAULT VALUES")
            return
        columns = ", ".join(map(identifier, values))
        placeholders = ", ".join("?" for _ in values)
        self._connection.execute(
            f"INSERT INTO {identifier(name)} ({columns}) VALUES ({placeholders})",
            parameters,
        )

    def _key(
        self, table: Table, key: Mapping[str, Value]
    ) -> tuple[str, tuple[SQLValue, ...]]:
        if set(key) != set(table.primary_key):
            raise ValueError("Mutation requires the complete primary key")
        where = " AND ".join(f"{identifier(name)} = ?" for name in key)
        return where, self._parameters(table, key)

    def update(
        self, name: str, key: Mapping[str, Value], values: Mapping[str, Value]
    ) -> None:
        """Update one key with the same strict input checks as insertion."""
        table = self._writable(name)
        if not values:
            raise ValueError("Empty update")
        where, key_values = self._key(table, key)
        parameters = self._parameters(table, values)
        assignments = ", ".join(f"{identifier(column)} = ?" for column in values)
        self._connection.execute(
            f"UPDATE {identifier(name)} SET {assignments} WHERE {where}",
            (*parameters, *key_values),
        )

    def delete(self, name: str, key: Mapping[str, Value]) -> None:
        """Delete by a complete key; referenced rows remain protected by RESTRICT."""
        table = self._writable(name)
        where, parameters = self._key(table, key)
        self._connection.execute(
            f"DELETE FROM {identifier(name)} WHERE {where}", parameters
        )

    def rows(self, name: str) -> tuple[Row, ...]:
        """Read declared columns in primary-key order and validate every stored value."""
        table = self._tables[name]
        columns = ", ".join(identifier(column.name) for column in table.columns)
        order = ", ".join(map(identifier, table.primary_key))
        raw = self._read(f"SELECT {columns} FROM {identifier(name)} ORDER BY {order}")
        return tuple(
            Row(
                name,
                MappingProxyType(
                    {
                        column.name: decode(column, value, self._rules)
                        for column, value in zip(table.columns, row, strict=True)
                    }
                ),
            )
            for row in raw
        )


@contextmanager
def create_database(
    schema: CompiledSchema, path: Path | None = None
) -> Iterator[Database]:
    """Create a fresh build DB, or a private in-memory DB; never overwrite a file."""
    if path is not None:
        with path.open("xb"):
            pass
    connection = sqlite3.connect(
        ":memory:" if path is None else path, isolation_level=None
    )
    try:
        rules = install_functions(connection, schema)
        connection.execute("PRAGMA foreign_keys = ON")
        database = Database(connection, schema, rules)
        with database.transaction():
            for statement in schema.statements:
                connection.execute(statement)
        yield database
    finally:
        connection.close()
