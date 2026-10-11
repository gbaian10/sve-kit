"""Infer product families from product codes before consulting printing membership."""

from string import ascii_lowercase
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build import Database


def populate(db: Database) -> list[dict[str, JsonValue]]:
    """Use printing home sets, never the card's original set when handling reprints."""
    families = {str(row.values["id"]): row.values for row in db.rows("product_family")}
    codes = {
        str(row["code"]).casefold(): identifier for identifier, row in families.items()
    }
    printings = {
        row.values["id"]: row.values["home_set_id"] for row in db.rows("printing")
    }
    members: dict[str, set[str]] = {}
    for row in db.rows("printing_product"):
        members.setdefault(str(row.values["product_id"]), set()).add(
            str(printings[row.values["printing_id"]])
        )
    diagnostics: list[dict[str, JsonValue]] = []
    for row in db.rows("product"):
        product = row.values
        code = product["product_code"]
        parts = [] if code is None else str(code).casefold().split("-")
        selected = {codes[part] for part in parts if part in codes}
        if len(selected) != len(parts) or not parts:
            selected = set()
        if not selected and code is not None:
            prefix = str(code).casefold()
            selected = {
                identifier
                for family_code, identifier in codes.items()
                if family_code.startswith(prefix)
                and len(family_code) == len(prefix) + 1
                and family_code[-1] in ascii_lowercase
            }
        if not selected and product["family_id"] is not None:
            selected = {str(product["family_id"])}
        if not selected:
            selected = members.get(str(product["id"]), set())
        if not selected:
            diagnostics.append(
                {
                    "product_id": str(product["id"]),
                    "region": str(product["region"]),
                    "product_code": None if code is None else str(code),
                    "reason": "no_matching_code_or_printing_family",
                }
            )
        for family in sorted(selected):
            db.insert(
                "product_family_member",
                {"product_id": product["id"], "family_id": family},
            )
        updates = {"family_id": next(iter(selected)) if len(selected) == 1 else None}
        db.update("product", {"id": product["id"]}, updates)
    return diagnostics
