"""Default printings from normalized rarity and permanent home-set release order."""

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from sve_carddb.domains.registry.rarity import RARITY_ORDER
from sve_carddb.domains.routes.plan import confirmed, text

if TYPE_CHECKING:
    from sve_carddb.build.database import Database, Row


@dataclass(frozen=True)
class DefaultPrinting:
    card_id: str
    region: str
    printing_id: str
    method: Literal["override", "lowest_rarity", "candidate_lowest_rarity", "fallback"]


def _home_dates(db: Database) -> dict[tuple[str, str], str]:
    products = {text(row, "id"): row for row in db.rows("product")}
    dates: dict[tuple[str, str], str] = {}
    for member in db.rows("product_family_member"):
        product = products[text(member, "product_id")]
        if product.values["date_precision"] != "day":
            continue
        day = text(product, "released_on")
        key = text(member, "family_id"), text(product, "region")
        dates[key] = min(dates.get(key, day), day)
    return dates


def select_defaults(db: Database) -> tuple[DefaultPrinting, ...]:
    """Select within each card/region; unknown rarity sorts after ranked vocabulary codes."""
    ranks = {code: rank for rank, code in enumerate(RARITY_ORDER)}
    home_dates = _home_dates(db)
    groups: dict[tuple[str, str], list[Row]] = defaultdict(list)
    for row in db.rows("printing"):
        groups[text(row, "card_id"), text(row, "region")].append(row)
    overrides = {
        (text(row, "card_id"), text(row, "region")): row
        for row in db.rows("default_printing_override")
    }
    if overrides.keys() - groups.keys():
        raise ValueError("Default override has no card/region printing")

    unknown = len(ranks)

    def rank(row: Row) -> int:
        code = row.values["rarity_code"]
        if isinstance(code, str) and code in ranks:
            return ranks[code]
        # A bare premium label has no tier but is known not to be the plain printing.
        return unknown + 1 if row.values["premium"] is True else unknown

    def order(row: Row) -> tuple[int, bool, str, str, str]:
        day = home_dates.get((text(row, "home_set_id"), text(row, "region")))
        return rank(row), day is None, day or "", text(row, "card_no"), text(row, "id")

    result = []
    for key, candidates in sorted(groups.items()):
        override = overrides.get(key)
        if override is not None:
            confirmed(db, text(override, "decision_id"))
            selected = text(override, "printing_id")
            if selected not in {text(item, "id") for item in candidates}:
                raise ValueError(
                    "Default override must select the same card and region"
                )
            result.append(DefaultPrinting(*key, selected, "override"))
            continue
        selected_row = min(candidates, key=order)
        method: Literal["lowest_rarity", "candidate_lowest_rarity", "fallback"] = (
            "fallback"
            if rank(selected_row) >= unknown
            else "candidate_lowest_rarity"
            if any(rank(item) >= unknown for item in candidates)
            else "lowest_rarity"
        )
        result.append(DefaultPrinting(*key, text(selected_row, "id"), method))
    return tuple(result)
