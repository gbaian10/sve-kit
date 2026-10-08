"""Derive stable entries from checked build rows without identity repair."""

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from sve_carddb.routes.codec import card_path

if TYPE_CHECKING:
    from sve_carddb.build.database import Database, Row

Namespace = Literal["official", "provisional"]


def text(row: Row, column: str) -> str:
    """Narrow a validated build value without letting SQLite Any cross the boundary."""
    value = row.values[column]
    if not isinstance(value, str):
        raise TypeError("Expected build text")
    return value


@dataclass(frozen=True, order=True)
class Route:
    namespace: Namespace
    key: str
    printing_id: str

    @property
    def path(self) -> str:
        """The exact encoded canonical entry."""
        return card_path(self.namespace, self.key)


def confirmed(db: Database, decision_id: str) -> None:
    """Require a real confirmed decision, never official-source inference."""
    decisions = {text(row, "id"): text(row, "state") for row in db.rows("decision")}
    if decisions.get(decision_id) != "confirmed":
        raise ValueError("Route/default override or alias is not confirmed")


def derive_routes(db: Database) -> tuple[Route, ...]:
    """Refuse exact cross-region collisions and ambiguous same-region variants."""
    allocations = {
        text(row, "printing_id"): row.values["int_id"] for row in db.rows("card_int_id")
    }
    overrides = {text(row, "route_key"): row for row in db.rows("route_override")}
    official: dict[str, list[Row]] = defaultdict(list)
    routes: list[Route] = []
    for row in db.rows("printing"):
        printing_id = text(row, "id")
        allocated = allocations.get(printing_id)
        if type(allocated) is not int or not 0 < allocated <= 2**32 - 1:
            raise ValueError("Printing lacks a permanent UInt32 allocation")
        if text(row, "card_no_state") == "official":
            key = text(row, "card_no")
            card_path("official", key)
            official[key].append(row)
        else:
            routes.append(Route("provisional", str(allocated), printing_id))
    if overrides.keys() - official.keys():
        raise ValueError("Route override has no official number")
    for key, variants in sorted(official.items()):
        if len({text(row, "region") for row in variants}) != 1:
            raise ValueError("Cross-region exact card number collision")
        override = overrides.get(key)
        if override is None:
            if len(variants) != 1:
                raise ValueError(
                    "Same-number variants require a confirmed route override"
                )
            selected = text(variants[0], "id")
        else:
            confirmed(db, text(override, "decision_id"))
            selected = text(override, "printing_id")
            if selected not in {text(row, "id") for row in variants}:
                raise ValueError(
                    "Route override does not select an exact-number variant"
                )
        routes.append(Route("official", key, selected))
    return tuple(sorted(routes))


def populate_canonical_routes(db: Database) -> tuple[Route, ...]:
    """Populate within the caller transaction; never replace an existing entry."""
    routes = derive_routes(db)
    existing = {
        (text(row, "namespace"), text(row, "route_key")): text(row, "printing_id")
        for row in db.rows("card_route")
    }
    expected: dict[tuple[str, str], str] = {
        (route.namespace, route.key): route.printing_id for route in routes
    }
    if any(expected.get(key) != value for key, value in existing.items()):
        raise ValueError("Existing canonical routes require identity-repair review")
    for route in routes:
        if (route.namespace, route.key) not in existing:
            db.insert(
                "card_route",
                {
                    "namespace": route.namespace,
                    "route_key": route.key,
                    "printing_id": route.printing_id,
                },
            )
    return routes
