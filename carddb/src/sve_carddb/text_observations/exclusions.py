"""Audit the transitive exclusion of existing staged references for preview output."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

if TYPE_CHECKING:
    from sve_carddb.build_db import CompiledSchema, Database, Row, Table, Value
    from sve_carddb.registry.records import Region
    from sve_carddb.text_observations.plan import TextPlan


def exclusion_report(
    db: Database, schema: CompiledSchema, plan: TextPlan
) -> dict[str, JsonValue]:
    """Close regional seeds over all enabled DB foreign keys, including optional uses."""
    tables = {table.name: table for table in schema.tables}
    rows = {name: db.rows(name) for name in tables}
    excluded: dict[str, set[tuple[Value, ...]]] = {name: set() for name in tables}
    _identity_seeds(tables, rows, excluded, plan)
    allowed: set[tuple[str, Region]] = set()
    for item in plan.eligible.projections:
        if item.disposition == "included" and item.record_key.startswith("face:"):
            allowed.update(
                (item.record_key.removeprefix("face:"), region)
                for region in item.regions
            )
    for name in ("face_revision", "face_current"):
        for row in rows[name]:
            if (row.values["face_id"], row.values["region"]) not in allowed:
                excluded[name].add(
                    tuple(row.values[column] for column in tables[name].primary_key)
                )
    _close(tables, rows, excluded)
    return {
        "excluded_row_counts": {
            name: len(keys) for name, keys in sorted(excluded.items())
        },
        "excluded_primary_keys": {
            name: [_key_json(key) for key in sorted(keys)]
            for name, keys in sorted(excluded.items())
            if keys
        },
    }


def _key_json(key: tuple[Value, ...]) -> list[JsonValue]:
    values: list[JsonValue] = []
    for value in key:
        if not isinstance(value, (str, int)) and value is not None:
            raise ValueError("Non-scalar build primary key")
        values.append(value)
    return values


def _identity_seeds(
    tables: dict[str, Table],
    rows: dict[str, tuple[Row, ...]],
    excluded: dict[str, set[tuple[Value, ...]]],
    plan: TextPlan,
) -> None:
    indices = {
        name: {
            tuple(row.values[column] for column in table.primary_key)
            for row in rows[name]
        }
        for name, table in tables.items()
    }
    for old, new in zip(
        plan.identity.projections, plan.eligible.projections, strict=True
    ):
        if old.record_key != new.record_key:
            raise ValueError("Identity closure record order mismatch")
        if old.disposition == "included" and new.disposition != "included":
            record = plan.identity.snapshot.records[old.record_key]
            if record.kind in tables:
                data = record.entry().data
                values = tuple(
                    data[column] for column in tables[record.kind].primary_key
                )
                if any(not isinstance(value, (str, int)) for value in values):
                    raise ValueError("Non-scalar identity primary key")
                key = tuple(value for value in values if isinstance(value, (str, int)))
                if key in indices[record.kind]:
                    excluded[record.kind].add(key)


def _close(
    tables: dict[str, Table],
    rows: dict[str, tuple[Row, ...]],
    excluded: dict[str, set[tuple[Value, ...]]],
) -> None:
    changed = True
    while changed:
        changed = False
        for name, table in tables.items():
            for fk in table.foreign_keys:
                if fk.table not in tables:
                    continue
                target = tables[fk.table]
                missing = {
                    tuple(row.values[column] for column in fk.target)
                    for row in rows[fk.table]
                    if tuple(row.values[column] for column in target.primary_key)
                    in excluded[fk.table]
                }
                for row in rows[name]:
                    key = tuple(row.values[column] for column in table.primary_key)
                    if (
                        key not in excluded[name]
                        and tuple(row.values[column] for column in fk.columns)
                        in missing
                    ):
                        excluded[name].add(key)
                        changed = True
