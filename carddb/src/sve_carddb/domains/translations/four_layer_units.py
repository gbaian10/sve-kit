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
    CountContext(
        "select.designation.v1", "designation", (), False, "cardinality"
    ): frozenset({"つ"}),
}
for _kind in ("follower", "spell", "amulet", "card", "spell_or_amulet", "crest"):
    for _zone in ("deck", "evolve_deck", "graveyard", "hand", "banished"):
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
for _kind in ("follower", "amulet", "card"):
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
_RULES[
    CountContext(
        "select.union_unrestricted.v1",
        "card",
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
    "クレスト": "crest",
}
_ZONES = {
    "場": "battlefield",
    "EXエリア": "ex",
    "エボルヴデッキ": "evolve_deck",
    "デッキ": "deck",
    "墓場": "graveyard",
    "手札": "hand",
    "消滅領域": "banished",
}
_TRAIT = (
    r"(?:(?!自分|相手|フォロワー|アミュレット|スペル|カード|EXエリア|墓場|手札|デッキ|N|の)"
    r"[^、。:『』{}「」\s・0-9０-９])+・"
)
_FILTER = (
    r"(?:(?:元の)?(?:コスト|攻撃力|体力)(?:N|X)(?:以下|以上)?の|"
    r"他の|表向きの|裏向きの|(?:進化前|進化後|エボルヴ|アドバンス)(?:の)?|(?:アクト|レスト|スタンド)状態の|これと同名を除く(?:・)?|"
    r"(?:【[^【】]+】(?:や|か)?)+を持つ|カード名に『X』を含む|カード名に「[^「」]+」を含む|"
    r"\{[^{}]+\}(?:でない|の|・)?|" + _TRAIT + r")*"
)
_ONSET = r"(?:^|[、。:：}】（(]|か|と|は|として)"
_KIND = r"スペルかアミュレット|フォロワー|アミュレット|スペル|カード|クレスト|『X』"
_SET_KIND = r"(?:" + _KIND + r")(?:(?:や|か|と)" + _FILTER + r"(?:" + _KIND + r"))*"
_SET_ATOM = re.compile(_FILTER + r"(?P<kind>" + _KIND + r")")
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
    + r"(?:(?:自分|相手)の)?(?P<zone>場とEXエリア|場か自分のEXエリア|場か相手のEXエリア|場や自分のEXエリア|場や相手のEXエリア|エボルヴデッキ|EXエリア|消滅領域|デッキ|墓場|手札|場)"
    r"(?:の|にある|にいる|に表向きで置かれている|に裏向きで置かれている|に表向きで置いた|に裏向きで置いた|に表向きである|に裏向きである|に|から)(?:、)?(?P<quote>「)?"
    + _FILTER
    + r"(?P<kind>"
    + _SET_KIND
    + r")(?(quote)」)(?:を|が)?$"
)
_EX_TOKEN = re.compile(
    _ONSET
    + r"(?:自分|相手)のEXエリア(?:の|にある|にいる)トークン・フォロワー(?:を|が)?$"
)
_IMPLICIT_FIELD = re.compile(
    _ONSET
    + _FILTER
    + r"(?:自分|相手)の"
    + r"(?P<quote>「)?"
    + _FILTER
    + r"(?P<kind>フォロワー|アミュレット)(?(quote)」)(?:を|が)?$"
)
_LEADER = re.compile(r"(?:自分|相手)のリーダー(?:を|が)?$")
_PLAYER = re.compile(r"(?:自分|相手)プレイヤー(?:を|が)?$")
_COLLECTION = re.compile(
    _ONSET
    + r"(?:(?:自分|相手|それ(?:のプレイヤー)?)の)?(?:現在の)?(?P<zone>墓場|手札|消滅領域)(?:が|に)?$"
)
_LOOK_SELECTION = re.compile(
    _ONSET + r"(?:自分|相手)のデッキの上(?:から)?N枚(?:を)?見(?:る|て)[。、]"
    r"(?:その中から|その中の)(?:、)?(?P<quote>「)?"
    + _FILTER
    + r"(?P<kind>"
    + _SET_KIND
    + r")?(?(quote)」)(?:を|が)?$"
)
_FIELD_FILTER = re.compile(
    r"^(?:の)?(?:(?:自分|相手)の(?:(?:場|墓場|手札|デッキ)の)?)?"
    + _FILTER
    + r"(?:"
    + _KIND
    + r")(?=N|が|を|の|」|すべて|それぞれ|なら|でない|や|か|と|[、。]|$)"
)
_COMPOUND_SELECTION = re.compile(
    r"^(?P<unit>枚|体|つ|人)(?P<limit>まで)?(?:か|と)"
    r"(?P<next>[^。:：]+)N(?:枚|体|つ|人)(?:まで)?(?:を)?"
    r"選(?:ぶ|び|んで)(?=[。:、）)\n]|$)"
)
_SHARED_SET = re.compile(
    r"^(?P<quote>「)?"
    + _FILTER
    + r"(?P<kind>"
    + _SET_KIND
    + r")(?(quote)」)(?:を|が)?$"
)
_COMPOUND_PREFIX = re.compile(
    r"^(?P<first>.*)N(?P<unit>枚|体|つ)(?:まで)?(?:か|と)(?P<next>[^。:：]+)$"
)
_ACTION_SERIES = re.compile(
    r"^(?P<unit>枚|体|つ)(?P<limit>まで)?(?:か|と)(?P<tail>.+)$"
)
_SERIES_MEMBER = re.compile(
    r"^(?P<np>[^。:：]+?)N(?P<unit>枚|体|つ)(?:まで)?(?P<tail>.*)$"
)
_SERIES_ACTION = re.compile(
    r"^(?:を)?(?:捨てる|消滅|墓場に置く|手札に戻す|"
    r"探し、(?:手札に加える|場に出す|墓場に置く|EXエリアに置く)|"
    r"公開して手札に加えてよい)(?=[。:：、）)\n]|$)"
)


_SET_CONSTRAINT = re.compile(
    r"を(?:(?:カード名|クラス|元のコスト)が異なるように|"
    r"元のコストの合計が(?:N|X)以下になるように)$"
)


def field_filter(after: str) -> bool:
    """Known filters must terminate in an explicit kind before classifying a field value."""
    return _FIELD_FILTER.match(after) is not None


def compound_selection(after: str, before: str = "") -> tuple[str, bool] | None:
    """Both counted NPs must be explicit; the final selection verb belongs to both."""
    match = _COMPOUND_SELECTION.match(after)
    if match is None:
        return None
    second = count_context(match["next"])
    if second is None and (first := count_context(before)) is not None:
        second = _shared_set(first, match["next"])
    if second is None:
        return None
    return match["unit"], bool(match["limit"])


def compound_action(after: str, before: str) -> tuple[str, bool] | None:
    """Every coordinate count shares the final action only after all NPs and units validate."""
    start = _ACTION_SERIES.match(after)
    first = count_context(before)
    if (
        start is None
        or first is None
        or not source_unit(first, start["unit"]).merge_allowed
    ):
        return None
    tail = start["tail"]
    while member := _SERIES_MEMBER.match(tail):
        context = count_context(member["np"]) or _shared_set(first, member["np"])
        if context is None or not source_unit(context, member["unit"]).merge_allowed:
            return None
        tail = member["tail"]
        if _SERIES_ACTION.match(tail):
            return start["unit"], bool(start["limit"])
        if not tail.startswith(("か", "と")):
            return None
        first, tail = context, tail[1:]
    return None


def count_context(before: str) -> CountContext | None:
    """A complete counted NP supplies its zone; the later destination is never inspected."""
    if before.endswith("それぞれ"):
        before = before.removesuffix("それぞれ").removesuffix("が")
    if constraint := _SET_CONSTRAINT.search(before):
        return count_context(before[: constraint.start()])
    if match := _COMPOUND_PREFIX.match(before):
        first = count_context(match["first"])
        if (
            first is not None
            and source_unit(first, match["unit"]).merge_allowed
            and (shared := _shared_set(first, match["next"])) is not None
        ):
            return shared
    if context := _explicit_object_context(before):
        return context
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


def _shared_set(first: CountContext, tail: str) -> CountContext | None:
    """Only an adjacent complete coordinate NP shares its written collection introducer."""
    match = _SHARED_SET.fullmatch(tail)
    if match is None or len(first.counted_zones) != 1:
        return None
    zone = first.counted_zones[0]
    if zone in {"battlefield", "ex"} and "『X』" in match["kind"]:
        return None
    return CountContext(
        "select.ex_unrestricted.v1" if zone == "ex" else "select.unrestricted.v1",
        _counted_kind(match["kind"]),
        (zone,),
        _ALL_TOKENS,
        "cardinality",
    )


def _explicit_object_context(before: str) -> CountContext | None:
    if re.search(_ONSET + r"(?:カード名|好きな数)$", before):
        return CountContext(
            "select.designation.v1", "designation", (), False, "cardinality"
        )
    if _LEADER.search(before):
        return CountContext("select.leader.v1", "leader", (), False, "player_count")
    if _PLAYER.search(before):
        return CountContext("select.player.v1", "player", (), False, "player_count")
    if _EX_TOKEN.search(before):
        return CountContext("select.card.v1", "follower", ("ex",), True, "cardinality")
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
            _counted_kind(match["kind"]),
            ("deck",),
            _ALL_TOKENS,
            "cardinality",
        )
    match = _COUNTED.search(before)
    return _counted_context(match) if match is not None else None


def _counted_context(match: re.Match[str]) -> CountContext | None:
    kind = _counted_kind(match["kind"])
    if match["zone"] in {
        "場とEXエリア",
        "場か自分のEXエリア",
        "場か相手のEXエリア",
        "場や自分のEXエリア",
        "場や相手のEXエリア",
    }:
        return CountContext(
            "select.union_unrestricted.v1",
            kind,
            ("battlefield", "ex"),
            _ALL_TOKENS,
            "union_cardinality",
        )
    zone = _ZONES[match["zone"]]
    if (zone in {"battlefield", "ex"} and "『X』" in match["kind"]) or (
        zone == "ex" and "トークン・" in match[0]
    ):
        return None
    if "トークン・" in match[0] and match["kind"] in {
        "フォロワー",
        "アミュレット",
        "カード",
    }:
        return CountContext("select.card.v1", kind, (zone,), True, "cardinality")
    return CountContext(
        "select.ex_unrestricted.v1" if zone == "ex" else "select.unrestricted.v1",
        kind,
        (zone,),
        _ALL_TOKENS,
        "cardinality",
    )


def _counted_kind(text: str | None) -> str:
    if text is None:
        return "card"
    kinds = set()
    while text:
        atom = _SET_ATOM.match(text)
        if atom is None:
            raise ValueError("Counted set must consist of complete known NPs")
        kinds.add(_KINDS.get(atom["kind"], "card"))
        text = text[atom.end() :]
        if text:
            if text[0] not in {"や", "か", "と"}:
                raise ValueError("Counted set must use a registered union connector")
            text = text[1:]
    return kinds.pop() if len(kinds) == 1 else "card"


def source_unit(context: CountContext, raw_unit: str) -> UnitDecision:
    """An unknown construction stays pending; a known construction's wrong unit cannot merge."""
    allowed = _RULES.get(context)
    if allowed is None:
        return UnitDecision(False, "source_unit_rule_unresolved")
    if raw_unit not in allowed:
        return UnitDecision(False, "source_unit_mismatch")
    return UnitDecision(True, None)
