"""Store expanded admitted programs separately from execution qualification."""

from pydantic import JsonValue

from sve_carddb.build.model import Check, Column, ForeignKey, Kind, Table, Unique
from sve_carddb.build.scalars import HASH
from sve_carddb.contracts.profiles import PROGRAMS
from sve_carddb.contracts.snapshot import schema

TABLES = (
    Table(
        "dsl_document",
        (
            Column("id", Kind.ID),
            Column("card_id", Kind.ID),
            Column("scope", Kind.TEXT, choices=("shared",)),
            Column("dsl_version", Kind.TEXT, choices=("1.0",)),
            Column(
                "status",
                Kind.TEXT,
                choices=("draft", "reviewed", "verified", "disputed"),
            ),
            Column("ast", Kind.JSON, json_schema="DSLProgram"),
            Column("program_hash", Kind.TEXT, pattern=HASH),
        ),
        ("id",),
        foreign_keys=(ForeignKey(("card_id",), "card", ("id",)),),
        unique=(Unique(("card_id", "scope")),),
        checks=(Check("json_extract(ast, '$.card_id') = card_id"),),
    ),
)


def schemas() -> dict[str, dict[str, JsonValue]]:
    """Use the same authoritative program definition at the storage boundary."""
    selected = schema(PROGRAMS).copy()
    selected.pop("$id")
    selected.pop("oneOf")
    return {"DSLProgram": selected | {"$ref": "#/$defs/DSLProgram"}}
