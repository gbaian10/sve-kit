"""Exact, alias and unambiguous folded lookup over the stable build entries."""

import hashlib
from collections import defaultdict
from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING, Literal

from sve_carddb.routes.codec import RESERVED, card_path, decode_segment, folded_key
from sve_carddb.routes.plan import (
    Route,
    confirmed,
    derive_routes,
    populate_canonical_routes,
    text,
)

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build.database import Database


@dataclass(frozen=True)
class Resolution:
    status: Literal["exact", "redirect", "ambiguous", "missing", "reserved"]
    printing_id: str | None = None
    canonical_path: str | None = None


@dataclass(frozen=True)
class RouteIndex:
    routes: Mapping[tuple[str, str], Route]
    aliases: Mapping[tuple[str, str], Route]
    folded: Mapping[str, tuple[Route, ...]]
    collisions: tuple[str, ...]

    def resolve(self, path: str) -> Resolution:
        """Resolve a card path; an optional trailing slug does not affect identity."""
        parts = path.split("/")
        prefix = ["", "cards"]
        if len(parts) <= len(prefix) or parts[: len(prefix)] != prefix:
            raise ValueError("Expected a card path")
        key = decode_segment(parts[2])
        namespace = "official"
        if key == "unimplemented":
            return Resolution("reserved")
        if key == "_provisional":
            if len(parts) <= len(prefix) + 1:
                return Resolution("reserved")
            namespace, key = "provisional", decode_segment(parts[3])
            card_path(namespace, key)
        route = self.routes.get((namespace, key))
        if route is not None:
            return Resolution("exact", route.printing_id, route.path)
        route = self.aliases.get((namespace, key))
        if route is not None:
            return Resolution("redirect", route.printing_id, route.path)
        matches = (
            self.folded.get(folded_key(key), ()) if namespace == "official" else ()
        )
        if len(matches) == 1:
            return Resolution("redirect", matches[0].printing_id, matches[0].path)
        return Resolution("ambiguous" if matches else "missing")


def build_index(db: Database) -> RouteIndex:
    """Validate canonical rows and adopted aliases; report disabled folded keys."""
    canonical: dict[tuple[str, str], Route] = {
        (route.namespace, route.key): route for route in derive_routes(db)
    }
    actual = {
        (text(row, "namespace"), text(row, "route_key")): text(row, "printing_id")
        for row in db.rows("card_route")
    }
    if actual != {key: route.printing_id for key, route in canonical.items()}:
        raise ValueError("Canonical routes differ from printing identities")
    aliases: dict[tuple[str, str], Route] = {}
    for row in db.rows("card_route_alias"):
        key = text(row, "namespace"), text(row, "old_key")
        card_path(*key)
        if key in canonical:
            raise ValueError("Alias cannot hijack an active canonical entry")
        target = text(row, "target_namespace"), text(row, "target_key")
        if target not in canonical:
            raise ValueError("Alias must directly target a current canonical entry")
        confirmed(db, text(row, "decision_id"))
        aliases[key] = canonical[target]
    folded: dict[str, set[Route]] = defaultdict(set)
    for (namespace, route_key), route in (*canonical.items(), *aliases.items()):
        if namespace == "official" and folded_key(route_key) not in RESERVED:
            folded[folded_key(route_key)].add(route)
    entries = {key: tuple(sorted(value)) for key, value in sorted(folded.items())}
    return RouteIndex(
        MappingProxyType(canonical),
        MappingProxyType(aliases),
        MappingProxyType(entries),
        tuple(key for key, value in entries.items() if len(value) > 1),
    )


def populate_routes(db: Database) -> tuple[Route, ...]:
    """Populate and validate atomically; preserve exact routes while warning on folds."""
    routes = populate_canonical_routes(db)
    index = build_index(db)
    issues = {text(row, "id"): row.values for row in db.rows("build_issue")}
    for key in index.collisions:
        issue_id = "route-folded:" + hashlib.sha256(key.encode("utf-8")).hexdigest()
        values = {
            "id": issue_id,
            "category": "route_folded_collision",
            "severity": "warning",
            "entity_type": "card_route",
            "entity_id": key,
            "message": "Ambiguous folded key is disabled; exact entries remain available.",
            "source_id": None,
        }
        if issue_id in issues:
            if dict(issues[issue_id]) != values:
                raise ValueError("Conflicting folded route diagnostic")
        else:
            db.insert("build_issue", values)
    return routes
