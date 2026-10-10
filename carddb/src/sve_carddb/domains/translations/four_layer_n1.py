"""Closed N1 body operands; local lexical success never resolves an effect."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import Span

if TYPE_CHECKING:
    from collections.abc import Iterator


ZONES = {
    "エボルヴデッキ": "evolve_deck",
    "EXエリア": "ex",
    "消滅領域": "banish",
    "デッキ": "deck",
    "墓場": "graveyard",
    "手札": "hand",
    "場": "battlefield",
}
KINDS = {"フォロワー": "follower", "アミュレット": "amulet", "スペル": "spell"}
PLAYERS = {"自分": "self", "相手": "opponent"}
PHASES = {
    "スタートフェイズ": "start",
    "メインフェイズ": "main",
    "エンドフェイズ": "end",
}
_ZONE = "(?:" + "|".join(ZONES) + ")"
_OWNER = r"(?:(?P<owner>自分|相手)の)?"
_LOCATION = re.compile(_OWNER + r"(?P<zone>" + _ZONE + r")(?P<case>の|から|に)")
_EDGE = re.compile(_OWNER + r"(?P<position>デッキの(?P<edge>上|下))")
_PHASE = re.compile(
    _OWNER + r"(?P<phase>" + "|".join(PHASES) + r")(?P<case>が来たとき|に)"
)
_QUOTED = re.compile(r"『[^』]*』|「[^「」。:：]*」|\{[^{}]*\}|【[^【】]*】")
_NP_TOKEN = re.compile(
    r"(?P<name>『X』)|(?P<quote>「[^「」。:：]*」)(?:の)?|"
    r"(?P<kind>フォロワー|アミュレット|スペル)|(?P<card>カード)|"
    r"(?P<token>トークン)・?|(?P<braced>\{[^{}]+\})(?:の|・)?|"
    r"(?P<fixed>他の|表向きの|裏向きの|(?:元の)?(?:コスト|攻撃力|体力)N(?:以上|以下)?の|"
    r"(?:進化前|進化後|エボルヴ|アドバンス)の|(?:アクト|レスト|スタンド)状態の)|"
    r"(?P<trait>[^、。:：『』{}「」【】\s・]+)・"
)
_Q = r"(?P<number>N)(?P<unit>枚|体|つ)(?P<limit>まで)?"
_SELECT = re.compile(
    r"(?P<np>.+?)(?P<pre>を)?"
    + _Q
    + r"(?P<post>を)?(?P<verb>選ぶ|選び|選んで)(?=$|[。、])"
)
_READ = re.compile(
    r"(?P<np>.+?)"
    + _Q
    + r"(?:を)?(?P<verb>探す|探し|探して|見る|見て|公開する|公開して)(?=$|[。、]|手札に)"
)
_MOVE = re.compile(
    r"(?P<np>.+?)"
    + _Q
    + r"を(?P<destination>(?:自分の|相手の)?"
    + _ZONE
    + r"に)(?P<verb>置く|置き|置いて|戻す|戻し|戻して|加える|加え|加えて|出す|出し|出して)(?:よい)?(?=$|[。、])"
)
_DEST_VERBS = {
    "place": r"置く|置き|置いて|置いてよい",
    "return": r"戻す|戻し|戻して|戻してよい",
    "add_hand": r"加える|加え|加えてよい",
    "deploy": r"出す|出し|出して|出してよい",
}
_DEST = re.compile(
    _OWNER
    + r"(?P<zone>"
    + _ZONE
    + r")に(?P<verb>"
    + "|".join(_DEST_VERBS.values())
    + r")(?=$|[。、])"
)
_EDGE_SOURCE = re.compile(
    _Q
    + r"を(?P<verb>見る|見て|公開する|公開して|(?:自分の|相手の)?"
    + _ZONE
    + r"に(?:置く|置き|置いて|置いてよい|戻す|戻し|戻して|戻してよい))(?=$|[。、])"
)
_EDGE_DEST = re.compile(
    r"に(?:置く|置き|置いて|置いてよい|戻す|戻し|戻して|戻してよい)(?=$|[。、])"
)
_COUNT = re.compile(
    r"(?P<np>.+?)(?P<tail>の(?:枚数|数)|が(?:ちょうど)?N(?:枚|体|つ)(?:以上|以下)?なら(?:使える)?|が(?:いる|いない|ある|ない)(?:なら|場合)?)(?=$|[。、]|だけ|が|と|に|を|の|分|＋|×)"
)
_UNION = re.compile(
    r"(?P<owner>自分|相手)の(?P<first>場|EXエリア)(?P<connector>か|や)"
    r"(?:(?P<second_owner>自分|相手)の)?(?P<second>場|EXエリア)の"
)
_HAND_COST = re.compile(
    r"(?:(?P<owner>自分|相手)の)?(?P<zone>手札)(?:の(?P<np>.+?)|を)?"
    + _Q
    + r"(?:を)?捨てる(?=:)"
)
_COST_HEAD = re.compile(
    r"\{(?:起動|起動能力|ファンファーレ|ラストワード|進化|食事|憑依|アドバンス起動)\}(?P<cost>(?:\{(?:コストN|アクト|レスト)\}|、)*)$"
)
_PARENTS = re.compile(
    r"出たとき|置かれたとき|戻ったとき|加えたとき|捨てたとき|プレイしたとき|攻撃したとき|破壊されたとき|消滅したとき|出るたび|置かれるたび|このターン中に|ターン中|場にいる限り|場にある限り|いたなら|履歴|受け取った|出たカード|置いたカード"
)


@dataclass(frozen=True)
class Filter:
    kind: str
    spelling: str
    span: Span


@dataclass(frozen=True)
class Noun:
    span: Span
    kind: str
    filters: tuple[Filter, ...]
    opaque: bool


@dataclass(frozen=True)
class Operand:
    row: str
    span: Span
    owners: tuple[Span, ...] = ()
    zone_spans: tuple[Span, ...] = ()
    zones: tuple[str, ...] = ()
    position: Span | None = None
    edge: str | None = None
    phase: Span | None = None
    phase_code: str | None = None
    noun: Noun | None = None
    number: Span | None = None
    unit: str | None = None
    mode: str = "exact"
    verb: str = ""
    particle: Span | None = None
    pre_particle: Span | None = None
    post_particle: Span | None = None

    @property
    def role(self) -> str:
        """The row, rather than an observed code, fixes every operand's direction."""
        if self.row.startswith(("N1-SRC03", "N1-SRC06.destination")):
            return "destination"
        if self.row.startswith("N1-SRC04"):
            return "counted"
        return "source"


@dataclass(frozen=True)
class Attempt:
    row: str
    accepted: bool


def _span(match: re.Match[str], group: str, offset: int = 0) -> Span:
    return Span(start=offset + match.start(group), end=offset + match.end(group))


def noun(text: str, offset: int) -> Noun | None:
    """Opaque modifiers terminate at their own token boundary, never at a later verb."""
    cursor = 0
    filters = []
    kind = None
    opaque = False
    while cursor < len(text):
        token = _NP_TOKEN.match(text, cursor)
        if token is None or kind is not None:
            return None
        group = token.lastgroup
        if group == "kind":
            kind = KINDS[token[group]]
        elif group in {"name", "card"} or (
            group == "quote" and token.end() == len(text)
        ):
            kind = "card"
        opaque |= group in {"fixed", "quote"}
        if group in {"kind", "card", "trait", "token", "braced"}:
            filters.append(Filter(group, token[group], _span(token, group, offset)))
        cursor = token.end()
    if kind is None:
        return None
    return Noun(
        Span(start=offset, end=offset + len(text)), kind, tuple(filters), opaque
    )


def _bounded_start(text: str, start: int) -> bool:
    if start == 0:
        return True
    return text[start - 1] in "、。:：}】をはでとに" or text[:start].endswith(
        ("次の", "この", "公開して", "探して", "選んで")
    )


def _local_end(text: str, start: int) -> int:
    protected = tuple(_QUOTED.finditer(text))
    for index in range(start, len(text)):
        if text[index] in "。:：" and not any(
            m.start() <= index < m.end() for m in protected
        ):
            return index + (text[index] == "。")
    return len(text)


def _parent_excluded(text: str, start: int, end: int) -> bool:
    first = max(text.rfind("。", 0, start), text.rfind(":", 0, start)) + 1
    local = text[first:end]
    local = _QUOTED.sub("", local)
    return _PARENTS.search(local) is not None


def _np_action(
    text: str, start: int, pattern: re.Pattern[str]
) -> tuple[re.Match[str], Noun] | None:
    end = _local_end(text, start)
    match = pattern.match(text[:end], start)
    if match is None:
        return None
    parsed = noun(match["np"], match.start("np"))
    return (match, parsed) if parsed is not None else None


def operands(text: str) -> tuple[tuple[Operand, ...], tuple[Attempt, ...]]:
    """Claim complete positions and unions before considering single-zone candidates."""
    registry = _Registry(text)
    registry.unions()
    registry.positions()
    registry.sources()
    registry.counts()
    registry.destinations()
    registry.phases()
    registry.costs()
    return tuple(registry.found), tuple(registry.attempts)


class _Registry:
    def __init__(self, text: str) -> None:
        self.text = text
        self.found: list[Operand] = []
        self.attempts: list[Attempt] = []
        self.blocked: list[Span] = []
        self.protected = tuple(_QUOTED.finditer(text))

    def allowed(self, start: int, end: int) -> bool:
        return (
            _bounded_start(self.text, start)
            and not any(m.start() <= start < m.end() for m in self.protected)
            and not any(s.start <= start < s.end for s in self.blocked)
            and not _parent_excluded(self.text, start, end)
        )

    def unions(self) -> None:
        text, found, attempts = self.text, self.found, self.attempts
        for match in _UNION.finditer(text):
            if any(m.start() <= match.start() < m.end() for m in self.protected):
                continue
            end = _local_end(text, match.end())
            self.blocked.append(Span(start=match.start(), end=end))
            action = _np_action(text, match.end(), _SELECT)
            accepted = bool(
                action is not None
                and match["first"] != match["second"]
                and match["second_owner"] in {None, match["owner"]}
                and _bounded_start(text, match.start())
                and not any(
                    m.start() <= match.start() < m.end() for m in self.protected
                )
                and not _parent_excluded(text, match.start(), end)
            )
            row = (
                "N1-SRC10."
                + ("repeated" if match["second_owner"] else "shared")
                + "."
                + match["connector"]
                + "."
                + ZONES[match["first"]]
            )
            attempts.append(Attempt(row, accepted))
            if accepted:
                assert action is not None
                tail, np = action
                found.append(_selection(row, match, tail, np, union=True))
        # Reject the right half of every unregistered coordinate scope as well.
        for match in re.finditer(
            _ZONE
            + r"(?:の[^。:：、]*?)?(?:と|または|および|及び|か|や)(?:自分の|相手の)?"
            + _ZONE,
            text,
        ):
            if not any(m.start() <= match.start() < m.end() for m in self.protected):
                self.blocked.append(
                    Span(start=match.start(), end=_local_end(text, match.end()))
                )

    def positions(self) -> None:
        text, found, attempts = self.text, self.found, self.attempts
        for match in _EDGE.finditer(text):
            end = _local_end(text, match.end())
            if not self.allowed(match.start(), end):
                continue
            tail = _EDGE_SOURCE.match(text[:end], match.end())
            dest = _EDGE_DEST.match(text[:end], match.end())
            row = (
                "N1-SRC06.source.inspect"
                if tail is not None
                and tail["verb"] in {"見る", "見て", "公開する", "公開して"}
                else "N1-SRC06.source.move"
                if tail is not None
                else "N1-SRC06.destination"
            )
            attempts.append(Attempt(row, tail is not None or dest is not None))
            if tail is None and dest is None:
                continue
            stop = (
                tail.end()
                if tail is not None
                else dest.end()
                if dest is not None
                else match.end()
            )
            found.append(
                Operand(
                    row,
                    Span(start=match.start(), end=stop),
                    (_span(match, "owner"),) if match["owner"] else (),
                    position=_span(match, "position"),
                    edge="top" if match["edge"] == "上" else "bottom",
                    number=_span(tail, "number") if tail is not None else None,
                    unit=tail["unit"] if tail is not None else None,
                    verb=tail["verb"] if tail is not None else "return",
                )
            )
            self.blocked.append(_span(match, "position"))

    def sources(self) -> None:
        text, found, attempts = self.text, self.found, self.attempts
        for match in _LOCATION.finditer(text):
            end = _local_end(text, match.end())
            if not self.allowed(match.start(), end):
                continue
            case = match["case"]
            if case == "に":
                continue
            action = _np_action(text, match.end(), _SELECT)
            if action is not None:
                tail, np = action
                row = "N1-SRC01.select"
                found.append(_selection(row, match, tail, np))
                attempts.append(Attempt(row, True))
                continue
            action = _np_action(text, match.end(), _READ)
            if action is not None:
                tail, np = action
                row = "N1-SRC01." + (
                    "search"
                    if tail["verb"].startswith("探")
                    else "inspect"
                    if tail["verb"].startswith("見")
                    else "reveal"
                )
                found.append(_selection(row, match, tail, np))
                attempts.append(Attempt(row, True))
                continue
            action = _np_action(text, match.end(), _MOVE)
            if action is not None:
                tail, np = action
                row = "N1-SRC02." + ("locative" if case == "の" else "ablative")
                found.append(_selection(row, match, tail, np))
                attempts.append(Attempt(row, True))

    def counts(self) -> None:
        text, found, attempts = self.text, self.found, self.attempts
        for match in _LOCATION.finditer(text):
            end = _local_end(text, match.end())
            if not self.allowed(match.start(), end):
                continue
            action = _np_action(text, match.end(), _COUNT)
            if action is None:
                continue
            tail, np = action
            suffix = tail["tail"]
            family = (
                "read"
                if suffix.startswith("の")
                else "quantity"
                if "N" in suffix
                else "exist"
            )
            row = "N1-SRC04." + family
            accepted = match["case"] == ("に" if family == "exist" else "の")
            attempts.append(Attempt(row, accepted))
            if accepted:
                found.append(
                    Operand(
                        row,
                        Span(start=match.start(), end=tail.end()),
                        (_span(match, "owner"),) if match["owner"] else (),
                        (_span(match, "zone"),),
                        (ZONES[match["zone"]],),
                        noun=np,
                    )
                )

    def destinations(self) -> None:
        text, found, attempts = self.text, self.found, self.attempts
        for match in _DEST.finditer(text):
            end = _local_end(text, match.end())
            if not self.allowed(match.start(), end):
                continue
            zone = ZONES[match["zone"]]
            verb = match["verb"]
            family = (
                "add_hand"
                if verb.startswith("加")
                else "deploy"
                if verb.startswith("出")
                else "return"
                if verb.startswith("戻")
                else "place"
            )
            row = "N1-SRC03." + family
            accepted = family not in {"add_hand", "deploy"} or zone == (
                "hand" if family == "add_hand" else "battlefield"
            )
            attempts.append(Attempt(row, accepted))
            if accepted:
                found.append(
                    Operand(
                        row,
                        Span(start=match.start(), end=match.end()),
                        (_span(match, "owner"),) if match["owner"] else (),
                        (_span(match, "zone"),),
                        (zone,),
                        verb=family,
                    )
                )

    def phases(self) -> None:
        text, found, attempts = self.text, self.found, self.attempts
        for match in _PHASE.finditer(text):
            end = _local_end(text, match.end())
            row = "N1-SRC05." + (
                "arrival" if match["case"] == "が来たとき" else "delayed"
            )
            predicate = re.search(
                r"(?:引く|選ぶ|置く|戻す|加える|出す|与える|回復する|破壊する|消滅させる)(?:。|$)",
                text[match.end() : end],
            )
            accepted = self.allowed(match.start(), end) and (
                row.endswith("arrival") or predicate is not None
            )
            attempts.append(Attempt(row, accepted))
            if accepted:
                found.append(
                    Operand(
                        row,
                        Span(start=match.start(), end=match.end()),
                        (_span(match, "owner"),) if match["owner"] else (),
                        phase=_span(match, "phase"),
                        phase_code=PHASES[match["phase"]],
                    )
                )

    def costs(self) -> None:
        text, found, attempts = self.text, self.found, self.attempts
        for match in _HAND_COST.finditer(text):
            row = "N1-SRC02.hand_cost"
            head = _COST_HEAD.search(text[: match.start()])
            np = noun(match["np"], match.start("np")) if match["np"] else None
            accepted = head is not None and (match["np"] is None or np is not None)
            attempts.append(Attempt(row, accepted))
            if accepted:
                found.append(
                    Operand(
                        row,
                        Span(start=match.start(), end=match.end()),
                        (_span(match, "owner"),) if match["owner"] else (),
                        (_span(match, "zone"),),
                        ("hand",),
                        noun=np,
                        number=_span(match, "number"),
                        unit=match["unit"],
                        verb="discard",
                    )
                )


def _selection(
    row: str,
    location: re.Match[str],
    tail: re.Match[str],
    np: Noun,
    *,
    union: bool = False,
) -> Operand:
    owners: tuple[Span, ...] = (_span(location, "owner"),) if location["owner"] else ()
    if union and location["second_owner"]:
        owners += (_span(location, "second_owner"),)
    zones = (
        (_span(location, "first"), _span(location, "second"))
        if union
        else (_span(location, "zone"),)
    )
    codes = ("battlefield", "ex") if union else (ZONES[location["zone"]],)
    return Operand(
        row,
        Span(start=location.start(), end=tail.end()),
        owners,
        zones,
        codes,
        noun=np,
        number=_span(tail, "number"),
        unit=tail["unit"],
        mode="up_to" if tail["limit"] else "exact",
        verb=tail["verb"],
        particle=_span(location, "case") if not union else None,
        pre_particle=_span(tail, "pre")
        if "pre" in tail.re.groupindex and tail["pre"]
        else None,
        post_particle=_span(tail, "post")
        if "post" in tail.re.groupindex and tail["post"]
        else None,
    )


_INSPECTED_SELECTION = re.compile(
    r"。(?:その中から|その中の)、?(?P<np>[^。:：]+?)N枚(?:を)?公開して"
)


def _self_source(text: str, operand: Operand) -> bool:
    return (
        bool(operand.owners)
        and text[operand.owners[0].start : operand.owners[0].end] == "自分"
    )


def _inspected_selection(gap: str) -> bool:
    match = _INSPECTED_SELECTION.fullmatch(gap)
    return match is not None and not re.search(
        r"選|捨|置|戻|相手|プレイヤー|それを", match["np"]
    )


def _hand_destination(text: str, operand: Operand, found: tuple[Operand, ...]) -> bool:
    previous = [
        o
        for o in found
        if o.span.end <= operand.span.start
        and o.role == "source"
        and o.row.startswith(("N1-SRC01", "N1-SRC06.source"))
    ]
    if not previous:
        return False
    source = max(previous, key=lambda o: o.span.end)
    if not _self_source(text, source):
        return False
    gap = text[source.span.end : operand.span.start]
    return (
        (
            source.verb in {"探し", "探して", "公開して", "選び", "選んで"}
            and re.fullmatch(r"(?:、)?(?:それを)?", gap) is not None
        )
        or (source.verb == "選ぶ" and gap == "。それを")
        or (source.row == "N1-SRC06.source.inspect" and _inspected_selection(gap))
    )


def _return_deck(text: str, operand: Operand, found: tuple[Operand, ...]) -> bool:
    previous = [
        o
        for o in found
        if o.row == "N1-SRC06.source.inspect" and o.span.end < operand.span.start
    ]
    if len(previous) != 1 or not _self_source(text, previous[0]):
        return False
    gap = text[previous[0].span.end : operand.span.start]
    if re.fullmatch(r"。残りを(?:好きな順に)?", gap) is not None:
        return True
    suffix = "手札に加えてよい。残りを好きな順に"
    return gap.endswith(suffix) and _inspected_selection(gap.removesuffix(suffix))


def omitted_owners(
    text: str, found: tuple[Operand, ...]
) -> Iterator[tuple[Operand, str, str]]:
    """Only complete, unique self-source constructions license a new owner use."""
    for operand in found:
        if operand.owners:
            continue
        if operand.row == "N1-SRC02.hand_cost":
            yield operand, "owner.actor_hand_cost.v1", "actor_hand_cost"
        elif operand.row == "N1-SRC03.add_hand" and _hand_destination(
            text, operand, found
        ):
            previous = max(
                (
                    o
                    for o in found
                    if o.role == "source" and o.span.end <= operand.span.start
                ),
                key=lambda o: o.span.end,
            )
            shape = (
                "inspected"
                if previous.row == "N1-SRC06.source.inspect"
                else "selected"
                if previous.verb == "選ぶ"
                else "local"
            )
            yield (
                operand,
                "owner.self_hand_destination.v1",
                "self_hand_destination." + shape,
            )
        elif (
            operand.row == "N1-SRC06.destination"
            or (operand.row == "N1-SRC03.place" and operand.zones == ("deck",))
        ) and _return_deck(text, operand, found):
            yield (
                operand,
                "owner.return_inspected_deck.v1",
                "return_inspected_deck." + ("position" if operand.position else "zone"),
            )
