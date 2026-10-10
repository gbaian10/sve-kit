"""Source classifier units depend on a recognized construction and its complete context."""

import re
from dataclasses import dataclass

# An unrestricted source NP includes both states; absence is not a non-token claim.
_ALL_TOKENS = frozenset({False, True})


@dataclass(frozen=True)
class CountContext:
    constructor: str
    counted_object: str
    counted_zones: tuple[str, ...]
    token: bool | frozenset[bool]
    quantity_role: str

    def __post_init__(self) -> None:
        """Destination zones and inferred token status cannot replace counted-set evidence."""
        if self.counted_zones != tuple(sorted(set(self.counted_zones))):
            raise ValueError("Counted zones must be ordered and unique")
        token: object = self.token
        if type(token) is not bool and (
            not isinstance(token, frozenset)
            or token != _ALL_TOKENS
            or any(type(t) is not bool for t in token)
        ):
            raise TypeError(
                "Token context must be an explicit boolean or both token states"
            )


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
for _kind in ("follower", "spell", "amulet", "card", "spell_or_amulet"):
    for _zone in ("deck", "evolve_deck", "graveyard", "hand"):
        _RULES[
            CountContext("select.card.v1", _kind, (_zone,), False, "cardinality")
        ] = frozenset({"枚"})
    _RULES[
        CountContext(
            "select.ex_unrestricted.v1", _kind, ("ex",), _ALL_TOKENS, "cardinality"
        )
    ] = frozenset({"枚"})
_RULES[
    CountContext("select.card.v1", "card", ("battlefield",), False, "cardinality")
] = frozenset({"枚"})
for _kind in ("follower", "amulet"):
    _RULES[
        CountContext("select.card.v1", _kind, ("battlefield",), True, "cardinality")
    ] = _RULES[
        CountContext("select.card.v1", _kind, ("battlefield",), False, "cardinality")
    ]
for _context, _units in tuple(_RULES.items()):
    if _context.constructor == "select.card.v1" and _context.token is False:
        _RULES[
            CountContext(
                "select.unrestricted.v1",
                _context.counted_object,
                _context.counted_zones,
                _ALL_TOKENS,
                _context.quantity_role,
            )
        ] = _units
_RULES[
    CountContext(
        "select.union_unrestricted.v1",
        "follower",
        ("battlefield", "ex"),
        _ALL_TOKENS,
        "union_cardinality",
    )
] = frozenset({"枚"})

_KINDS = {
    "スペルかアミュレット": "spell_or_amulet",
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
_TRAIT = (
    r"(?:(?!自分|相手|フォロワー|アミュレット|スペル|カード|EXエリア|墓場|手札|デッキ|N|の)"
    r"[^、。:『』{}「」\s・0-9０-９])+・"
)
_FILTER = (
    r"(?:(?:元の)?(?:コスト|攻撃力|体力)N(?:以下|以上)?の|"
    r"他の|表向きの|裏向きの|(?:進化前|進化後)の|(?:アクト|レスト|スタンド)状態の|これと同名を除く・|"
    r"【[^【】]+】を持つ|カード名に『X』を含む|カード名に「[^「」]+」を含む|"
    r"\{[^{}]+\}(?:の|・)?|" + _TRAIT + r")*"
)
_ONSET = r"(?:^|[、。:：}】（(]|か|と)"
_KIND = r"スペルかアミュレット|フォロワー|アミュレット|スペル|カード|『X』"
_UNION_COUNTED = re.compile(
    _ONSET
    + _FILTER
    + r"(?:(?:自分|相手)の)?(?P<first>場|EXエリア)の"
    + _FILTER
    + r"(?P<first_kind>フォロワー)(?:や|と|か)"
    + r"(?:(?:自分|相手)の)?(?P<second>場|EXエリア)の"
    + _FILTER
    + r"(?P<second_kind>フォロワー)(?:を|が)?$"
)
_COUNTED = re.compile(
    _ONSET
    + _FILTER
    + r"(?:(?:自分|相手)の)?(?P<zone>場とEXエリア|場か自分のEXエリア|場か相手のEXエリア|エボルヴデッキ|EXエリア|デッキ|墓場|手札|場)"
    r"(?:の|にある|にいる|に|から)(?:、)?(?P<quote>「)?"
    + _FILTER
    + r"(?P<kind>"
    + _KIND
    + r")(?(quote)」)(?:を|が)?$"
)
_EX_TOKEN = re.compile(
    _ONSET
    + r"(?:自分|相手)のEXエリア(?:の|にある|にいる)トークン・フォロワー(?:を|が)?$"
)
_IMPLICIT_FIELD = re.compile(
    _ONSET
    + r"(?:自分|相手)の"
    + _FILTER
    + r"(?P<kind>フォロワー|アミュレット)(?:を|が)?$"
)
_LEADER = re.compile(r"(?:自分|相手)のリーダー(?:を|が)?$")
_PLAYER = re.compile(r"(?:自分|相手)プレイヤー(?:を|が)?$")
_COLLECTION = re.compile(
    r"(?:自分|相手|それ)の(?:現在の)?(?P<zone>墓場|手札)(?:が|に)$"
)
_LOOK_SELECTION = re.compile(
    _ONSET + r"(?:自分|相手)のデッキの上(?:から)?N枚(?:を)?見(?:る|て)[。、]"
    r"(?:その中から|その中の)(?:、)?(?P<quote>「)?"
    + _FILTER
    + r"(?P<kind>"
    + _KIND
    + r")(?(quote)」)(?:を|が)?$"
)
_FIELD_FILTER = re.compile(
    r"^(?:の)?(?:(?:自分|相手)の(?:場|墓場|手札|デッキ)の)?"
    + _FILTER
    + r"(?:"
    + _KIND
    + r")(?=N|が|を|の|」|[、。]|$)"
)
_COMPOUND_SELECTION = re.compile(
    r"^(?P<unit>枚|体|つ|人)(?P<limit>まで)?(?:か|と)"
    r"(?P<next>[^。:：]+)N(?:枚|体|つ|人)(?:まで)?(?:を)?"
    r"選(?:ぶ|び|んで)(?=[。:、）)\n]|$)"
)


def field_filter(after: str) -> bool:
    """Known filters must terminate in an explicit kind before classifying a field value."""
    return _FIELD_FILTER.match(after) is not None


def compound_selection(after: str) -> tuple[str, bool] | None:
    """Both counted NPs must be explicit; the final selection verb belongs to both."""
    match = _COMPOUND_SELECTION.match(after)
    if match is None or count_context(match["next"]) is None:
        return None
    return match["unit"], bool(match["limit"])


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
            "select.unrestricted.v1",
            _KINDS[match["kind"]],
            ("battlefield",),
            _ALL_TOKENS,
            "cardinality",
        )
    return None


def _zone_context(before: str) -> CountContext | None:
    if match := _UNION_COUNTED.search(before):
        zones = tuple(sorted({_ZONES[match["first"]], _ZONES[match["second"]]}))
        if zones == ("battlefield", "ex"):
            return CountContext(
                "select.union_unrestricted.v1",
                "follower",
                zones,
                _ALL_TOKENS,
                "union_cardinality",
            )
    if match := _COLLECTION.search(before):
        return CountContext(
            "select.unrestricted.v1",
            "card",
            (_ZONES[match["zone"]],),
            _ALL_TOKENS,
            "cardinality",
        )
    if match := _LOOK_SELECTION.search(before):
        return CountContext(
            "select.unrestricted.v1",
            _KINDS.get(match["kind"], "card"),
            ("deck",),
            _ALL_TOKENS,
            "cardinality",
        )
    match = _COUNTED.search(before)
    return _counted_context(match) if match is not None else None


def _counted_context(match: re.Match[str]) -> CountContext | None:
    kind = _KINDS.get(match["kind"], "card")
    if match["zone"] in {
        "場とEXエリア",
        "場か自分のEXエリア",
        "場か相手のEXエリア",
    }:
        return CountContext(
            "select.union_unrestricted.v1",
            kind,
            ("battlefield", "ex"),
            _ALL_TOKENS,
            "union_cardinality",
        )
    zone = _ZONES[match["zone"]]
    if (zone in {"battlefield", "ex"} and match["kind"] == "『X』") or (
        zone == "ex" and "トークン・" in match[0]
    ):
        return None
    if "トークン・" in match[0]:
        return CountContext("select.card.v1", kind, (zone,), True, "cardinality")
    return CountContext(
        "select.ex_unrestricted.v1" if zone == "ex" else "select.unrestricted.v1",
        kind,
        (zone,),
        _ALL_TOKENS,
        "cardinality",
    )


def source_unit(context: CountContext, raw_unit: str) -> UnitDecision:
    """An unknown construction stays pending; a known construction's wrong unit cannot merge."""
    allowed = _RULES.get(context)
    if allowed is None:
        return UnitDecision(False, "source_unit_rule_unresolved")
    if raw_unit not in allowed:
        return UnitDecision(False, "source_unit_mismatch")
    return UnitDecision(True, None)
