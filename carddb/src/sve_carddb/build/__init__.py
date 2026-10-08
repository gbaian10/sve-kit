"""Compile capability-scoped schemas and build fresh, validated SQLite databases."""

from sve_carddb.build.compiler import CompiledSchema, compile_schema
from sve_carddb.build.database import Database, Row, create_database
from sve_carddb.build.model import (
    Capability,
    Check,
    Column,
    ForeignKey,
    Json,
    Kind,
    QueryCheck,
    Table,
    Unique,
    Value,
)
from sve_carddb.build.rebuild import rebuild_database
from sve_carddb.build.registry import Registry

__all__ = [
    "Capability",
    "Check",
    "Column",
    "CompiledSchema",
    "Database",
    "ForeignKey",
    "Json",
    "Kind",
    "QueryCheck",
    "Registry",
    "Row",
    "Table",
    "Unique",
    "Value",
    "compile_schema",
    "create_database",
    "rebuild_database",
]
