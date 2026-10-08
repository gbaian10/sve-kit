"""Compile trusted declarations to deterministic STRICT SQLite DDL."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build.model import Column, Kind, QueryCheck, Table, identifier, literal
from sve_carddb.build.validation import BOUNDS, Rules
from sve_carddb.core.json import canonical

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build.registry import Registry


@dataclass(frozen=True)
class CompiledSchema:
    tables: tuple[Table, ...]
    statements: tuple[str, ...]
    json_schemas: tuple[tuple[str, str], ...]
    version: int = 1
    query_checks: tuple[QueryCheck, ...] = ()

    @property
    def sql(self) -> str:
        """Reproducible DDL; the connection must install CHECK functions."""
        return "\n\n".join(self.statements) + "\n"


def _columns(names: tuple[str, ...]) -> str:
    return ", ".join(map(identifier, names))


def _checks(column: Column) -> list[str]:
    name = identifier(column.name)
    checks: list[str] = []
    if column.kind in BOUNDS:
        low, high = BOUNDS[column.kind]
        checks.append(f"typeof({name}) = 'integer' AND {name} BETWEEN {low} AND {high}")
    if column.kind == Kind.BOOL:
        checks.append(f"{name} IN (0, 1)")
    if column.kind == Kind.ID:
        checks.append(f"length(CAST({name} AS BLOB)) > 0")
    if column.fixed is not None:
        checks.append(f"{name} = {literal(column.fixed)}")
    if column.choices:
        choices = ", ".join(map(literal, column.choices))
        checks.append(f"{name} IN ({choices})")
    if column.pattern is not None:
        checks.append(f"sve_fullmatch({literal(column.pattern)}, {name}) = 1")
    if column.json_schema is not None:
        checks.append(
            f"CASE WHEN json_valid({name}) THEN "
            f"sve_json_valid({literal(column.json_schema)}, {name}) = 1 ELSE 0 END"
        )
    return [
        f"CHECK ({name} IS NULL OR ({check}))"
        if column.nullable
        else f"CHECK ({check})"
        for check in checks
    ]


def _column(column: Column) -> str:
    storage = "INTEGER" if column.kind in {*BOUNDS, Kind.BOOL} else "TEXT"
    parts = [identifier(column.name), storage]
    if not column.nullable:
        parts.append("NOT NULL")
    if column.fixed is not None:
        parts.append("DEFAULT " + literal(column.fixed))
    return " ".join([*parts, *_checks(column)])


def _foreign_keys(table: Table, enabled: set[str]) -> list[str]:
    result: list[str] = []
    for key in table.foreign_keys:
        if key.table not in enabled:
            # A compound key can share a required face_id with a disabled art_id.
            nullable = [name for name in key.columns if table.column(name).nullable]
            if not nullable:
                raise ValueError("Required foreign target was not enabled")
            result.extend(f"CHECK ({identifier(name)} IS NULL)" for name in nullable)
        else:
            result.append(
                f"FOREIGN KEY ({_columns(key.columns)}) REFERENCES "
                f"{identifier(key.table)} ({_columns(key.target)}) "
                "ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED"
            )
    return result


def _table(table: Table, enabled: set[str]) -> list[str]:
    clauses = [_column(column) for column in table.columns]
    clauses.append(f"PRIMARY KEY ({_columns(table.primary_key)})")
    clauses.extend(
        f"UNIQUE ({_columns(unique.columns)})"
        for unique in table.unique
        if unique.where is None
    )
    clauses.extend(_foreign_keys(table, enabled))
    clauses.extend(f"CHECK ({check.sql})" for check in table.checks)
    body = ",\n  ".join(clauses)
    options = "STRICT, WITHOUT ROWID" if table.without_rowid else "STRICT"
    result = [f"CREATE TABLE {identifier(table.name)} (\n  {body}\n) {options};"]
    for index, unique in enumerate(table.unique):
        if unique.where is not None:
            name = identifier(f"uq_{table.name}_{index}")
            result.append(
                f"CREATE UNIQUE INDEX {name} ON {identifier(table.name)} "
                f"({_columns(unique.columns)}) WHERE {unique.where};"
            )
    return result


def compile_schema(
    registry: Registry,
    requested: tuple[str, ...],
    json_schemas: Mapping[str, JsonValue] | None = None,
    *,
    version: int = 1,
) -> CompiledSchema:
    """Compile capability closure without inventing declarations for future tables."""
    if type(version) is not int or not 1 <= version <= 2**31 - 1:
        raise ValueError(
            "Build schema version must be a positive signed 32-bit integer"
        )
    tables = registry.resolve(requested)
    schemas = {
        name: canonical(value).decode() for name, value in (json_schemas or {}).items()
    }
    Rules(schemas)
    for table in tables:
        for column in table.columns:
            if column.json_schema is not None and column.json_schema not in schemas:
                raise ValueError(f"Undefined JSON schema: {column.json_schema}")
    enabled = {table.name for table in tables}
    return CompiledSchema(
        tables,
        (
            f"PRAGMA user_version = {version};",
            *(statement for table in tables for statement in _table(table, enabled)),
        ),
        tuple(sorted(schemas.items())),
        version,
        tuple(check for table in tables for check in table.query_checks),
    )
