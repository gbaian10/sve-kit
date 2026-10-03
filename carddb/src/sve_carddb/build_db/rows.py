"""Idempotent typed row insertion shared by transaction-local importers."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.build_db import Database, Value


def insert_exact(
    db: Database, table: str, values: dict[str, Value], keys: tuple[str, ...]
) -> None:
    """Reject conflicting repeat inserts instead of silently replacing prior evidence."""
    previous = [
        row.values
        for row in db.select(
            table, db.columns(table), where={key: values[key] for key in keys}
        )
    ]
    if previous:
        if previous != [values]:
            raise ValueError("Conflicting catalog row")
    else:
        db.insert(table, values)
