"""Read explicitly requested build columns; no SQLite values escape its boundary."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_db import Json
from sve_carddb.core.json import string

if TYPE_CHECKING:
    from collections.abc import Iterable

    from sve_carddb.build_db import Database


type Record = dict[str, JsonValue]
type View = dict[str, list[Record]]


class Source:
    def __init__(self, db: Database) -> None:
        self.db = db
        self.cache: dict[tuple[str, tuple[str, ...]], list[Record]] = {}
        self.indexes: dict[tuple[str, str], dict[str, Record]] = {}
        self.joins: dict[
            tuple[str, str, tuple[str, ...]], dict[tuple[str, ...], list[Record]]
        ] = {}

    def rows(self, table: str, fields: str) -> list[Record]:
        """An absent capability has no rows; a malformed enabled one fails."""
        if not self.db.has_table(table):
            return []
        selected = tuple(fields.split(","))
        key = table, selected
        if key not in self.cache:
            self.cache[key] = [
                {
                    name: value.value if isinstance(value, Json) else value
                    for name, value in row.values.items()
                }
                for row in self.db.select(table, selected)
            ]
        return self.cache[key]

    def index(self, table: str, fields: str) -> dict[str, Record]:
        """Index a declared string primary key without coercion."""
        key = table, fields
        if key not in self.indexes:
            self.indexes[key] = {
                string(row["id"]): row for row in self.rows(table, fields)
            }
        return self.indexes[key]

    def matching(self, table: str, fields: str, **key: JsonValue) -> list[Record]:
        """Join typed rows on the complete caller-provided owner key."""
        names = tuple(key)
        marker = table, fields, names
        if marker not in self.joins:
            grouped: dict[tuple[str, ...], list[Record]] = {}
            for row in self.rows(table, fields):
                values = tuple(string(row[name]) for name in names)
                grouped.setdefault(values, []).append(row)
            self.joins[marker] = grouped
        return self.joins[marker].get(
            tuple(string(value) for value in key.values()), []
        )

    def url(self, identifier: JsonValue) -> JsonValue:
        """Resolve only the public URL, never raw locator or receipt metadata."""
        if identifier is None:
            return None
        return self.index("source_record", "id,url")[string(identifier)]["url"]

    def review(self, identifier: JsonValue) -> str:
        """Map adopted decision state to its public review level."""
        if identifier is None:
            return "unreviewed"
        state = string(self.index("decision", "id,state")[string(identifier)]["state"])
        return (
            state
            if state in {"model_reviewed", "sampled", "confirmed"}
            else "unreviewed"
        )


def pick(row: Record, fields: str) -> Record:
    """Copy a fixed field list without retaining the source mapping."""
    return {field: row[field] for field in fields.split(",")}


def json_list(values: Iterable[JsonValue]) -> list[JsonValue]:
    """Copy covariant iterables into JSON arrays without casts or coercion."""
    return list(values)
