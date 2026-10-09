"""Source classifier units depend on a recognized construction and its complete context."""

from dataclasses import dataclass


@dataclass(frozen=True)
class CountContext:
    constructor: str
    counted_object: str
    counted_zones: tuple[str, ...]
    token: bool
    quantity_role: str

    def __post_init__(self) -> None:
        """Destination zones and inferred token status cannot replace counted-set evidence."""
        if self.counted_zones != tuple(sorted(set(self.counted_zones))):
            raise ValueError("Counted zones must be ordered and unique")
        if type(self.token) is not bool:
            raise TypeError("Token context must be an explicit boolean")


@dataclass(frozen=True)
class UnitDecision:
    merge_allowed: bool
    reason: str | None


# Each key is a source construction, not a global Japanese classifier equivalence.
_RULES: dict[CountContext, frozenset[str]] = {
    CountContext(
        "select.card.v1", "follower", ("battlefield",), False, "cardinality"
    ): frozenset({"体"}),
    CountContext("select.card.v1", "follower", ("ex",), True, "cardinality"): frozenset(
        {"体"}
    ),
    CountContext(
        "select.union.v1", "follower", ("battlefield", "ex"), False, "union_cardinality"
    ): frozenset({"枚"}),
    CountContext(
        "select.card.v1", "amulet", ("battlefield",), False, "cardinality"
    ): frozenset({"つ", "枚"}),
    CountContext("select.leader.v1", "leader", (), False, "player_count"): frozenset(
        {"人"}
    ),
}
for _kind in ("follower", "spell", "amulet", "card"):
    for _zone in ("deck", "evolve_deck", "graveyard", "hand"):
        _RULES[
            CountContext("select.card.v1", _kind, (_zone,), False, "cardinality")
        ] = frozenset({"枚"})


def source_unit(context: CountContext, raw_unit: str) -> UnitDecision:
    """An unknown construction stays pending; a known construction's wrong unit cannot merge."""
    allowed = _RULES.get(context)
    if allowed is None:
        return UnitDecision(False, "source_unit_rule_unresolved")
    if raw_unit not in allowed:
        return UnitDecision(False, "source_unit_mismatch")
    return UnitDecision(True, None)
