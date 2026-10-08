"""Resolve implemented capabilities and validate relational declarations."""

import re
from dataclasses import dataclass

from sve_carddb.build.model import Capability, Column, Kind, Table, identifier


def _names(names: tuple[str, ...]) -> None:
    if not names or len(names) != len(set(names)):
        raise ValueError("Expected nonempty unique names")
    for name in names:
        identifier(name)


def _column(column: Column) -> None:
    identifier(column.name)
    if not isinstance(column.kind, Kind):
        raise TypeError("Unknown column kind")
    if (column.json_schema is not None) != (column.kind == Kind.JSON):
        raise ValueError("JSON columns require exactly one named JSON schema")
    if column.choices or column.fixed is not None or column.pattern is not None:
        if column.kind not in {Kind.TEXT, Kind.ID}:
            raise ValueError("String refinements require a string column")
        if len(column.choices) != len(set(column.choices)):
            raise ValueError("Duplicate enum value")
        if column.pattern is not None:
            re.compile(column.pattern)


def _fixed(column: Column) -> None:
    if column.fixed is not None:
        if column.nullable:
            raise ValueError("Fixed columns must be non-nullable")
        if column.choices and column.fixed not in column.choices:
            raise ValueError("Fixed value outside enum")
        if (
            column.pattern is not None
            and re.fullmatch(column.pattern, column.fixed) is None
        ):
            raise ValueError("Fixed value outside pattern")
        if column.kind == Kind.ID and not column.fixed:
            raise ValueError("Fixed ID cannot be empty")


def _table(table: Table) -> None:
    identifier(table.name)
    _names(tuple(column.name for column in table.columns))
    _names(table.primary_key)
    for column in table.columns:
        _column(column)
        _fixed(column)
    for name in table.primary_key:
        if table.column(name).nullable:
            raise ValueError("Primary key cannot be nullable")
    for constraint in table.unique:
        _names(constraint.columns)
        for name in constraint.columns:
            table.column(name)
    for foreign in table.foreign_keys:
        identifier(foreign.table)
        _names(foreign.columns)
        _names(foreign.target)
        if len(foreign.columns) != len(foreign.target):
            raise ValueError("Foreign key arity mismatch")
        for name in foreign.columns:
            table.column(name)


def _foreign_keys(
    table: Table, tables: dict[str, Table], owners: dict[str, str]
) -> None:
    for check in table.query_checks:
        identifier(check.name)
        _names(check.tables)
        if not set(check.tables) <= owners.keys():
            raise ValueError("Unregistered query check dependency")
    for foreign in table.foreign_keys:
        if foreign.table not in owners:
            raise ValueError(f"Unregistered foreign target {foreign.table}")
        if foreign.table not in tables:
            continue
        target = tables[foreign.table]
        keys = {target.primary_key} | {
            item.columns for item in target.unique if item.where is None
        }
        if foreign.target not in keys:
            raise ValueError("Foreign target requires an unconditional unique key")
        for child, parent in zip(foreign.columns, foreign.target, strict=True):
            left, right = table.column(child), target.column(parent)
            textual = {Kind.TEXT, Kind.ID, Kind.JSON}
            if (left.kind in textual) != (right.kind in textual):
                raise ValueError("Foreign key storage types disagree")


@dataclass(frozen=True)
class Registry:
    """Register future table ownership without pretending its capability is usable."""

    tables: tuple[Table, ...]
    capabilities: tuple[Capability, ...]

    def validate(self) -> None:
        """Reject ambiguous ownership, invalid declarations and unresolved targets."""
        _names(tuple(capability.name for capability in self.capabilities))
        if len({table.name for table in self.tables}) != len(self.tables):
            raise ValueError("Duplicate table declaration")
        tables = {table.name: table for table in self.tables}
        caps = {cap.name: cap for cap in self.capabilities}
        owners: dict[str, str] = {}
        for cap in self.capabilities:
            _names(cap.tables)
            if not set(cap.requires) <= caps.keys():
                raise ValueError("Unknown capability dependency")
            for name in cap.tables:
                if name in owners:
                    raise ValueError("Table belongs to multiple capabilities")
                owners[name] = cap.name
                if cap.implemented and name not in tables:
                    raise ValueError("Implemented capability lacks table declaration")
        if not tables.keys() <= owners.keys():
            raise ValueError("Table has no capability owner")
        for table in self.tables:
            _table(table)
        for table in self.tables:
            _foreign_keys(table, tables, owners)

    def resolve(self, requested: tuple[str, ...]) -> tuple[Table, ...]:
        """Close explicit dependencies and non-nullable foreign keys, including cycles."""
        self.validate()
        caps = {cap.name: cap for cap in self.capabilities}
        tables = {table.name: table for table in self.tables}
        owners = {name: cap.name for cap in self.capabilities for name in cap.tables}
        enabled: set[str] = set()
        pending = list(requested)
        while pending:
            name = pending.pop()
            if name in enabled:
                continue
            if name not in caps or not caps[name].implemented:
                raise ValueError(f"Unknown or unimplemented capability: {name}")
            cap = caps[name]
            enabled.add(name)
            pending.extend(cap.requires)
            for table_name in cap.tables:
                table = tables[table_name]
                pending.extend(
                    owners[target]
                    for check in table.query_checks
                    for target in check.tables
                )
                pending.extend(
                    owners[fk.table]
                    for fk in table.foreign_keys
                    if not any(table.column(col).nullable for col in fk.columns)
                )
        return tuple(tables[name] for name in sorted(tables) if owners[name] in enabled)

    def require_usable(self, requested: tuple[str, ...]) -> None:
        """Reject DDL-only capabilities, including those introduced by closure."""
        tables = {table.name for table in self.resolve(requested)}
        for cap in self.capabilities:
            if tables.intersection(cap.tables) and not (
                cap.importer_ready and cap.validator_ready
            ):
                raise ValueError(f"Importer/validator unavailable: {cap.name}")
