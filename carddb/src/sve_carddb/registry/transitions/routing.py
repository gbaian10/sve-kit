"""Derive and preserve exact printing entries from independently pinned source facts."""

import re
from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING, Literal

from sve_carddb.registry.inputs import canonical
from sve_carddb.registry.records import AllocationData, PrintingData
from sve_carddb.registry.transitions.models import Alias, Route, RouteState, RouteUpdate
from sve_carddb.routes.codec import card_path
from sve_carddb.routes.plan import Route as PrintingRoute

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.registry.transitions.state import Entity


@dataclass(frozen=True)
class RouteFact:
    printing_id: str
    region: Literal["jp", "en"]
    state: Literal["official", "provisional", "unknown"]
    card_no: str | None
    source_version_id: str


@dataclass(frozen=True)
class RouteProjection:
    states: Mapping[str, RouteState]

    def resolve(self, namespace: str, key: str) -> PrintingRoute | None:
        """Aliases resolve directly to the latest canonical of the same printing."""
        card_path(namespace, key)
        for printing_id, state in self.states.items():
            keys = [(state.canonical.namespace, state.canonical.route_key)]
            keys.extend((a.namespace, a.route_key) for a in state.aliases)
            if (namespace, key) in keys:
                return PrintingRoute(
                    state.canonical.namespace, state.canonical.route_key, printing_id
                )
        return None


def derive(
    records: Mapping[str, Entity], facts: tuple[RouteFact, ...]
) -> dict[str, Route]:
    """Unknown numbers cannot become official; ambiguity requires another adapter.

    Facts must come from frozen sources, never from the proposed routes array.
    This core deliberately refuses exact-number variants without an adopted override.
    """
    printings = {
        data.id: data
        for item in records.values()
        if isinstance(data := item.data(), PrintingData)
    }
    allocations = {
        data.printing_id: data.int_id
        for item in records.values()
        if isinstance(data := item.data(), AllocationData)
    }
    if len({f.printing_id for f in facts}) != len(facts) or {
        f.printing_id for f in facts
    } != set(printings):
        raise ValueError("Route source facts must cover each printing exactly once")
    result: dict[str, Route] = {}
    for fact in facts:
        if re.fullmatch(r"src:v1:[0-9a-f]{64}", fact.source_version_id) is None:
            raise ValueError("Route source must identify an immutable source version")
        if fact.region != printings[fact.printing_id].region:
            raise ValueError("Route source region disagrees with printing")
        if fact.state == "unknown":
            raise ValueError("Unknown route source cannot prove an official number")
        if fact.state == "official":
            if fact.card_no is None:
                raise ValueError("Official route requires an exact source number")
            route = Route(namespace="official", route_key=fact.card_no)
        elif fact.state == "provisional":
            route = Route(
                namespace="provisional", route_key=str(allocations[fact.printing_id])
            )
        else:
            raise ValueError("Invalid route source state")
        result[fact.printing_id] = route
    if len({(r.namespace, r.route_key) for r in result.values()}) != len(result):
        raise ValueError("Exact route collision requires a confirmed route override")
    return result


def project(
    canonical_routes: dict[str, Route], previous: RouteProjection | None
) -> RouteProjection:
    """Carry all old keys forward and flatten them to the same printing forever."""
    states: dict[str, RouteState] = {}
    for printing_id, route in canonical_routes.items():
        old = previous.states.get(printing_id) if previous else None
        aliases = list(old.aliases) if old else []
        if old and old.canonical != route:
            if route.namespace == "provisional":
                raise ValueError(
                    "Apply cannot replace an official route with provisional"
                )
            aliases.append(
                Alias(
                    namespace=old.canonical.namespace,
                    route_key=old.canonical.route_key,
                    reason="provisional_corrected"
                    if old.canonical.namespace == "provisional"
                    else "renumbered",
                )
            )
        states[printing_id] = RouteState(
            canonical=route,
            aliases=tuple(
                sorted(aliases, key=lambda a: canonical(a.model_dump(mode="json")))
            ),
        )
    owners: dict[tuple[str, str], str] = {}
    for printing_id, state in states.items():
        for route in (state.canonical, *state.aliases):
            key = route.namespace, route.route_key
            if key in owners:
                raise ValueError(
                    "Route canonical/alias cannot hijack a permanent entry"
                )
            owners[key] = printing_id
    if previous and previous.states.keys() - states.keys():
        raise ValueError("Permanent printing routes cannot disappear")
    return RouteProjection(MappingProxyType(states))


def check_updates(
    before: RouteProjection, after: RouteProjection, updates: tuple[RouteUpdate, ...]
) -> None:
    """Require the complete expected projection, rejecting arbitrary alias overrides."""
    expected = tuple(
        RouteUpdate(printing_id=pid, before=before.states[pid], after=state)
        for pid, state in after.states.items()
        if pid in before.states and before.states[pid] != state
    )

    def order(update: RouteUpdate) -> bytes:
        return canonical(update.model_dump(mode="json"))

    if tuple(sorted(expected, key=order)) != updates:
        raise ValueError(
            "Transition routes differ from complete source-derived changes"
        )
