"""Compile capability-scoped schemas and build fresh, validated SQLite databases."""

from sve_carddb.build_db.compiler import CompiledSchema, compile_schema
from sve_carddb.build_db.database import Database, Row, create_database
from sve_carddb.build_db.model import (
    Capability,
    Check,
    Column,
    ForeignKey,
    Json,
    Kind,
    Table,
    Unique,
    Value,
)
from sve_carddb.build_db.registry import Registry

__all__ = [
    "Capability",
    "Check",
    "Column",
    "CompiledSchema",
    "Database",
    "ForeignKey",
    "Json",
    "Kind",
    "Registry",
    "Row",
    "Table",
    "Unique",
    "Value",
    "compile_schema",
    "create_database",
]
