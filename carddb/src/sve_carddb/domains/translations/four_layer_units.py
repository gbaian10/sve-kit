"""Source classifier units depend on a recognized construction and its complete context."""

import re
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
    CountContext("select.player.v1", "player", (), False, "player_count"): frozenset(
        {"人"}
    ),
}
for _kind in ("follower", "spell", "amulet", "card"):
    for _zone in ("deck", "evolve_deck", "graveyard", "hand"):
        _RULES[
            CountContext("select.card.v1", _kind, (_zone,), False, "cardinality")
        ] = frozenset({"枚"})

_KINDS = {
    "フォロワー": "follower",
    "アミュレット": "amulet",
    "スペル": "spell",
    "カード": "card",
}
_ZONES = {
    "場": "battlefield",
    "EXエリア": "ex",
    "エボルヴデッキ": "evolve_deck",
    "デッキ": "deck",
    "墓場": "graveyard",
    "手札": "hand",
}
_FILTER = r"(?:(?:元の)?(?:コスト|攻撃力|体力)N(?:以下|以上)?の|\{[^{}]+\}・?)*"
_COUNTED = re.compile(
    r"(?:自分|相手)(?:の)?(?P<zone>場とEXエリア|エボルヴデッキ|EXエリア|デッキ|墓場|手札|場)"
    r"(?:の|にある|にいる|に|から)"
    + _FILTER
    + r"(?P<kind>フォロワー|アミュレット|スペル|カード|『X』)(?:を|が)?$"
)
_EX_TOKEN = re.compile(
    r"(?:自分|相手)のEXエリア(?:の|にある|にいる)トークン・フォロワー(?:を|が)?$"
)
_IMPLICIT_FIELD = re.compile(
    r"(?:自分|相手)の(?P<kind>フォロワー|アミュレット)(?:を|が)?$"
)
_LEADER = re.compile(r"(?:自分|相手)のリーダー(?:を|が)?$")
_PLAYER = re.compile(r"(?:自分|相手)プレイヤー(?:を|が)?$")
_COLLECTION = re.compile(r"(?:自分|相手)の(?P<zone>墓場|手札)(?:が|に)$")
_LOOK_SELECTION = re.compile(
    r"(?:自分|相手)のデッキの上(?:から)?N枚(?:を)?見(?:る|て)[。、]"
    r"(?:その中から|その中の)"
    + _FILTER
    + r"(?P<kind>フォロワー|アミュレット|スペル|カード|『X』)(?:を|が)?$"
)


def count_context(before: str) -> CountContext | None:
    """A complete counted NP supplies its zone; the later destination is never inspected."""
    if _LEADER.search(before):
        return CountContext("select.leader.v1", "leader", (), False, "player_count")
    if _PLAYER.search(before):
        return CountContext("select.player.v1", "player", (), False, "player_count")
    if _EX_TOKEN.search(before):
        return CountContext("select.card.v1", "follower", ("ex",), True, "cardinality")
    if context := _zone_context(before):
        return context
    if match := _IMPLICIT_FIELD.search(before):
        return CountContext(
            "select.card.v1",
            _KINDS[match["kind"]],
            ("battlefield",),
            False,
            "cardinality",
        )
    return None


def _zone_context(before: str) -> CountContext | None:
    if match := _COLLECTION.search(before):
        return CountContext(
            "select.card.v1", "card", (_ZONES[match["zone"]],), False, "cardinality"
        )
    if match := _LOOK_SELECTION.search(before):
        return CountContext(
            "select.card.v1",
            _KINDS.get(match["kind"], "card"),
            ("deck",),
            False,
            "cardinality",
        )
    if match := _COUNTED.search(before):
        kind = _KINDS.get(match["kind"], "card")
        if match["zone"] == "場とEXエリア":
            return CountContext(
                "select.union.v1",
                kind,
                ("battlefield", "ex"),
                False,
                "union_cardinality",
            )
        zone = _ZONES[match["zone"]]
        if zone == "ex" or (zone == "battlefield" and match["kind"] == "『X』"):
            return None
        return CountContext(
            "select.card.v1",
            kind,
            (zone,),
            False,
            "cardinality",
        )
    return None


def source_unit(context: CountContext, raw_unit: str) -> UnitDecision:
    """An unknown construction stays pending; a known construction's wrong unit cannot merge."""
    allowed = _RULES.get(context)
    if allowed is None:
        return UnitDecision(False, "source_unit_rule_unresolved")
    if raw_unit not in allowed:
        return UnitDecision(False, "source_unit_mismatch")
    return UnitDecision(True, None)
