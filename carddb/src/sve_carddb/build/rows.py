"""Idempotent typed row insertion shared by transaction-local importers."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.build import Database, Value


def insert_exact(
    db: Database, table: str, values: dict[str, Value], keys: tuple[str, ...]
) -> None:
    """Reject conflicting repeat inserts instead of silently replacing prior evidence."""
    db.insert_exact(table, values, keys)
