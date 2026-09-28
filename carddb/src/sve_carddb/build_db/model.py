"""Immutable, code-authored declarations for build-time SQLite schemas."""

import re
from dataclasses import dataclass
from enum import StrEnum

from pydantic import JsonValue


class Kind(StrEnum):
    """Storage domains; semantic refinements belong to column/table checks."""

    TEXT = "text"
    ID = "id"
    INT = "int"
    UINT = "uint"
    UINT32 = "uint32"
    BOOL = "bool"
    JSON = "json"


@dataclass(frozen=True)
class Json:
    """Distinguish a JSON value (including JSON null) from SQL NULL."""

    value: JsonValue


type Value = str | int | bool | Json | None


@dataclass(frozen=True)
class Column:
    """Declare nullability explicitly; fixed strings become ordinary SQL columns."""

    name: str
    kind: Kind
    nullable: bool = False
    choices: tuple[str, ...] = ()
    fixed: str | None = None
    pattern: str | None = None
    json_schema: str | None = None


@dataclass(frozen=True)
class ForeignKey:
    columns: tuple[str, ...]
    table: str
    target: tuple[str, ...]


@dataclass(frozen=True)
class Unique:
    columns: tuple[str, ...]
    where: str | None = None


@dataclass(frozen=True)
class Check:
    """Hold trusted, code-authored SQL, never a value supplied by an importer."""

    sql: str


@dataclass(frozen=True)
class Table:
    name: str
    columns: tuple[Column, ...]
    primary_key: tuple[str, ...]
    foreign_keys: tuple[ForeignKey, ...] = ()
    unique: tuple[Unique, ...] = ()
    checks: tuple[Check, ...] = ()

    def column(self, name: str) -> Column:
        """Resolve a declared column, rejecting misspelled or unknown names."""
        for column in self.columns:
            if column.name == name:
                return column
        raise ValueError(f"Unknown column {self.name}.{name}")


@dataclass(frozen=True)
class Capability:
    name: str
    tables: tuple[str, ...]
    requires: tuple[str, ...] = ()
    implemented: bool = True


def identifier(name: str) -> str:
    """Quote a portable identifier without accepting SQL fragments."""
    if re.fullmatch(r"[a-z][a-z0-9_]*", name) is None or name.startswith("sqlite_"):
        raise ValueError(f"Invalid schema identifier: {name!r}")
    return '"' + name + '"'


def literal(value: str) -> str:
    """Quote declaration constants, including apostrophes, as SQL text."""
    value.encode("utf-8")
    if "\x00" in value:
        raise ValueError("NUL in declaration constant")
    return "'" + value.replace("'", "''") + "'"
