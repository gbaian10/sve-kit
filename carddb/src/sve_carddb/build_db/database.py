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
        self._version = schema.version
        self._query_checks = schema.query_checks

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
        if self._read("PRAGMA user_version") != ((self._version,),):
            raise sqlite3.IntegrityError("Build schema version mismatch")
        for check in self._query_checks:
            if self._read(check.sql):
                raise sqlite3.IntegrityError(f"Cross-table check failed: {check.name}")
        if "search_alias" in self._tables:
            self.verify_alias_targets()
        if "face_semantics" in self._tables:
            from sve_carddb.wording_adoptions.semantics import verify_semantics  # ruff: ignore[import-outside-top-level] -- semantic validation uses the initialized typed database boundary

            verify_semantics(self)

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

    def has_table(self, name: str) -> bool:
        """Report declared capability availability without probing SQLite internals."""
        return name in self._tables

    def columns(self, name: str) -> tuple[str, ...]:
        """Expose declaration names, never unvalidated SQLite schema metadata."""
        return tuple(column.name for column in self._tables[name].columns)

    def select(self, name: str, selected: tuple[str, ...]) -> tuple[Row, ...]:
        """Read only an explicit column whitelist through the typed boundary."""
        table = self._tables[name]
        if not selected or len(selected) != len(set(selected)):
            raise ValueError("Expected nonempty unique selected columns")
        declarations = tuple(table.column(column) for column in selected)
        columns = ", ".join(map(identifier, selected))
        order = ", ".join(map(identifier, table.primary_key))
        raw = self._read(f"SELECT {columns} FROM {identifier(name)} ORDER BY {order}")
        return tuple(
            Row(
                name,
                MappingProxyType(
                    {
                        column.name: decode(column, value, self._rules)
                        for column, value in zip(declarations, row, strict=True)
                    }
                ),
            )
            for row in raw
        )

    def alias_target_codes(self, kind: str) -> tuple[str, ...]:
        """Resolve only the fixed polymorphic target map inside the SQL boundary."""
        targets = {
            "keyword": ("keyword", "id"),
            "product_family": ("product_family", "code"),
            "stamp": ("stamp", "code"),
            "card": ("card", "id"),
        }
        if kind in targets:
            table, column = targets[kind]
            if table not in self._tables:
                return ()
            return tuple(str(row.values[column]) for row in self.rows(table))
        return tuple(
            str(row.values["code"])
            for row in self.rows("vocabulary")
            if row.values["kind"] == kind
        )

    def verify_alias_targets(self) -> None:
        """Verify polymorphic references with a whitelisted SQL union, including absent targets."""
        reserved = "('keyword','product_family','stamp','card')"
        queries = [f"SELECT kind, code FROM vocabulary WHERE kind NOT IN {reserved}"]
        for kind, table, column in (
            ("keyword", "keyword", "id"),
            ("product_family", "product_family", "code"),
            ("stamp", "stamp", "code"),
            ("card", "card", "id"),
        ):
            if table in self._tables:
                queries.append(
                    f"SELECT '{kind}', {identifier(column)} FROM {identifier(table)}"
                )
        targets = " UNION ALL ".join(queries)
        if self._read(
            f"SELECT 1 FROM search_alias AS a LEFT JOIN ({targets}) AS t ON a.kind=t.kind AND a.code=t.code WHERE t.code IS NULL"
        ):
            raise ValueError("Search alias has no valid target")

    def rows(self, name: str) -> tuple[Row, ...]:
        """Read declared columns in primary-key order and validate every stored value."""
        return self.select(name, self.columns(name))


@contextmanager
def open_database(schema: CompiledSchema, path: Path) -> Iterator[Database]:
    """Read a closed offline database without schema, WAL or journal writes."""
    if path.is_symlink() or not path.is_file():
        raise ValueError("Offline build must be an existing regular file")
    if any(
        path.with_name(path.name + suffix).exists()
        for suffix in ("-wal", "-shm", "-journal")
    ):
        raise ValueError("Offline build has SQLite sidecars; close it first")
    connection = sqlite3.connect(
        path.resolve().as_uri() + "?mode=ro&immutable=1", uri=True, isolation_level=None
    )
    try:
        rules = install_functions(connection, schema)
        connection.execute("PRAGMA foreign_keys = ON")
        connection.execute("PRAGMA query_only = ON")
        yield Database(connection, schema, rules)
    finally:
        connection.close()


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
