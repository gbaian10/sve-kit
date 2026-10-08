"""Synthetic DB inputs for route/default constraints, without official text."""

from typing import TYPE_CHECKING

from .build_db_fixtures import rows

if TYPE_CHECKING:
    from sve_carddb.build import Database, Value


def base(db: Database) -> dict[str, dict[str, Value]]:
    values = rows()
    with db.transaction():
        for name in (
            "source_record",
            "decision",
            "language",
            "text_unit",
            "product_family",
            "card",
            "face",
            "product",
        ):
            db.insert(name, values[name])
        for kind in ("rarity", "frame"):
            db.insert(
                "vocabulary", values["vocabulary"] | {"kind": kind, "code": "synthetic"}
            )
    return values


def printing(
    db: Database,
    pid: str,
    number: str,
    *,
    region: str = "jp",
    state: str = "official",
    changes: dict[str, Value] | None = None,
) -> None:
    values = rows()
    db.insert(
        "printing",
        values["printing"]
        | {"id": pid, "card_no": number, "region": region, "card_no_state": state}
        | (changes or {}),
    )
    allocated = (
        20001 + sum(row.values["region"] == "jp" for row in db.rows("printing"))
        if region == "jp"
        else 60001 + sum(row.values["region"] == "en" for row in db.rows("printing"))
    )
    db.insert(
        "card_int_id", values["card_int_id"] | {"int_id": allocated, "printing_id": pid}
    )
    db.insert("printing_face", values["printing_face"] | {"printing_id": pid})
