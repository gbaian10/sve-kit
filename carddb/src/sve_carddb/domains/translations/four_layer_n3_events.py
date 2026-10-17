"""Finite #749 event, dice and counter readers with explicit source providers."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    CapturedValueRef,
    EventRef,
    FaceRevisionOwner,
    ObjectSetRef,
    PlayerRef,
    ReceiptRef,
    Span,
    VocabularyReference,
)
from sve_carddb.domains.translations.four_layer_n3_ports import holder
from sve_carddb.domains.translations.four_layer_n3_shapes import EX_COUNTER_READ

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_n3_sources import Builder

_DAMAGE_THRESHOLD = 5
_DIE_MAX = 6

HOLDER_REVISIONS = frozenset(
    {
        "rev:v1:1eff100048852acfaf5c23647df5a4a5cd0c755a137f1149784fe25d7c2e59d0",
        "rev:v1:218f331babb65789632158a0eb46efc936bb3b4976152deded22ba9cf89f777e",
        "rev:v1:46cb13a3b5b261a0a111f3cfdd29be66d6e3213f79afb83b18ff61f7d9a6d7b0",
        "rev:v1:9bb466ce1cc6ace1aae00319b7496f0c0ec0f29f28346c736fb0ac5f9b14e937",
        "rev:v1:eef523d7ecaafad96c0b6966c72300f1c0acc5226cb582d019b685a5c4c3a3c9",
    }
)


def _span(match: re.Match[str], group: str) -> Span:
    return Span(start=match.start(group), end=match.end(group))


def _ex_counters(builder: Builder) -> bool:
    """3B is a maintainer interpretation confined to five immutable face revisions."""
    owner = builder.source.owner
    if (
        not isinstance(owner, FaceRevisionOwner)
        or owner.revision_id not in HOLDER_REVISIONS
    ):
        return False
    first = EX_COUNTER_READ.search(builder.text)
    if first is None:
        return False
    seconds = tuple(
        re.finditer(
            r"(?P<n>N)(?P<read>個)(?P<compare>以上)なら、", builder.text[first.end() :]
        )
    )
    if len(seconds) != 1:
        return False
    offset = first.end()
    second = seconds[0]

    def second_span(group: str) -> Span:
        return Span(start=offset + second.start(group), end=offset + second.end(group))

    if (
        builder.number(_span(first, "count")),
        builder.number(_span(first, "n")),
        builder.number(second_span("n")),
    ) != (1, 4, 10) or builder.scope(first.start()) != builder.scope(
        second_span("n").start
    ):
        return False
    counter = builder.term(
        _span(first, "counter"), "rule_term", "term:object.counter", "read_counter"
    )
    if counter is None:
        return False
    builder.numeric(first, "count", "count")
    provider = Span(start=first.start(), end=first.end("counter"))
    subject = builder.port(
        "highest_satisfied_holder",
        ObjectSetRef(kind="ObjectSet", objects=(holder(builder),), origin="selection"),
        "live",
        provider,
        first.start(),
    )
    for number, read, compare in [
        (_span(first, "n"), _span(first, "read"), _span(first, "compare")),
        (second_span("n"), second_span("read"), second_span("compare")),
    ]:
        value = builder.expression(
            "read.object.counter.v1",
            (counter,),
            (read,),
            {"subject": subject},
            builder.scope(first.start()),
        )
        threshold = builder.scalar(
            "Nat", "comparison_threshold", "", builder.number(number), (number,)
        )
        builder.expression(
            "condition.quantity.at_least.v1",
            (value, threshold),
            (compare,),
            {},
            builder.scope(first.start()),
        )
    return True


def _damage(builder: Builder) -> bool:
    match = re.search(
        r"(?P<trigger>これがダメージを受けたとき、)(?P<read>その)ダメージが(?P<n>N)(?P<compare>以上)なら、",
        builder.text,
    )
    if match is None or builder.number(_span(match, "n")) != _DAMAGE_THRESHOLD:
        return False
    span = _span(match, "trigger")
    subject = holder(builder)
    object_port = builder.port(
        "read_subject",
        ObjectSetRef(kind="ObjectSet", objects=(subject,), origin="event_subject"),
        "at_event",
        span,
        match.start(),
    )
    event = builder.port(
        "read_event",
        EventRef(
            kind="EventRef",
            event_kind="Damage",
            event_id=builder.identity("damage", span),
            subject=subject,
        ),
        "at_event",
        span,
        match.start(),
    )
    value = builder.expression(
        "read.event.damage_amount.v1",
        (),
        (_span(match, "read"),),
        {"subject": object_port, "event": event},
        builder.scope(match.start()),
    )
    threshold = builder.numeric(match, "n", "comparison_threshold")
    builder.expression(
        "condition.quantity.at_least.v1",
        (value, threshold),
        (_span(match, "compare"),),
        {},
        builder.scope(match.start()),
    )
    return True


def _destroyed(builder: Builder) -> bool:
    match = re.search(
        r"(?P<read>これ)によって破壊された自身の場の(?P<kind>フォロワー)(?P<n>N)体につき、",
        builder.text,
    )
    if match is None or builder.number(_span(match, "n")) != 1:
        return False
    resolution = builder.classifier.references.proposed_vocabulary("type", "フォロワー")
    if (
        resolution.issues
        or resolution.target is None
        or resolution.target.get("vocabulary_code") != "follower"
    ):
        return False
    operations = tuple(
        re.finditer(r"場の[^。]+すべてを破壊する。", builder.text[: match.start()])
    )
    if len(operations) != 1 or builder.scope(operations[0].start()) != builder.scope(
        match.start()
    ):
        return False
    operation = operations[0]
    span = Span(start=operation.start(), end=operation.end())
    kind = builder.scalar(
        "CardKind",
        "read_kind",
        "card_kind.selection.v1",
        VocabularyReference(kind="vocabulary", key=("type", "follower")),
        (_span(match, "kind"),),
    )
    receipt = builder.port(
        "read_receipt",
        ReceiptRef(
            kind="ReceiptId",
            result="objects",
            receipt_id=builder.identity("destroy", span),
        ),
        "receipt",
        span,
        match.start(),
    )
    player = builder.port(
        "read_player",
        PlayerRef(kind="PlayerRef", player="each_player_at_destroy"),
        "receipt",
        span,
        match.start(),
    )
    builder.expression(
        "read.result.destroyed_follower_by_player.v1",
        (kind,),
        (_span(match, "read"),),
        {"receipt": receipt, "player": player},
        builder.scope(match.start()),
    )
    builder.numeric(match, "n", "group_divisor")
    return True


def _dice_history(builder: Builder) -> bool:
    match = re.search(
        r"(?P<query>このターン、)(?P<owner>自分)がふった(?P<die>サイコロ)の出た目に(?P<face>N)があるなら、",
        builder.text,
    )
    if match is None or builder.number(_span(match, "face")) != _DIE_MAX:
        return False
    die = builder.term(_span(match, "die"), "rule_term", "term:object.die", "rule_term")
    if die is None:
        return False
    owner = builder.scalar(
        "Player",
        "history_owner",
        "player.relative.v1",
        "self",
        (_span(match, "owner"),),
    )
    face = builder.scalar("Nat", "dice_face", "", 6, (_span(match, "face"),), maximum=6)
    builder.additions[-1] = replace(
        builder.additions[-1],
        slot=builder.additions[-1].slot.model_copy(
            update={
                "domain": builder.additions[-1].slot.domain.model_copy(
                    update={"min": 1}
                )
            }
        ),
    )
    actor = builder.port(
        "history_actor",
        PlayerRef(kind="PlayerRef", player="self"),
        "current_turn",
        _span(match, "owner"),
        match.start(),
        (owner,),
    )
    builder.expression(
        "history.dice.face.exists.v1",
        (owner, face, die),
        (_span(match, "query"),),
        {"actor": actor},
        builder.scope(match.start()),
    )
    return True


def _dice_declaration(builder: Builder) -> bool:
    match = re.search(
        r"(?P<lower>N)から(?P<upper>N)の中から、好きな数(?P<selection>N)つを指定して(?P<die>サイコロ)を(?P<roll>ふる)。(?:出た目が)?(?P<compare>指定した数なら)、",
        builder.text,
    )
    if match is None or tuple(
        builder.number(_span(match, n)) for n in ("lower", "upper", "selection")
    ) != (1, 6, 1):
        return False
    die = builder.term(_span(match, "die"), "rule_term", "term:object.die", "rule_term")
    if die is None:
        return False
    lower = builder.numeric(match, "lower", "declaration_lower_bound")
    upper = builder.numeric(match, "upper", "declaration_upper_bound")
    selection = builder.numeric(match, "selection", "selection_count", "exact")
    span = Span(start=match.start(), end=match.end("selection"))
    declaration = builder.port(
        "declared_dice_face",
        CapturedValueRef(
            kind="CapturedValue",
            binding_id=builder.identity("dice-declaration", span),
            result_type="Nat",
            measure="declared_dice_face",
        ),
        "declaration",
        span,
        match.start(),
    )
    roll = _span(match, "roll")
    builder.port(
        "source_actor",
        PlayerRef(kind="PlayerRef", player="self"),
        "live",
        span,
        match.start(),
    )
    event = builder.port(
        "dice_event",
        EventRef(
            kind="EventRef",
            event_kind="DiceRoll",
            event_id=builder.identity("dice-roll", roll),
            actor=PlayerRef(kind="PlayerRef", player="self"),
        ),
        "at_event",
        roll,
        match.start(),
    )
    result = builder.port(
        "dice_result",
        CapturedValueRef(
            kind="CapturedValue",
            binding_id=builder.identity("dice-result", roll),
            result_type="Nat",
            measure="die_face",
        ),
        "receipt",
        roll,
        match.start(),
    )
    builder.expression(
        "read.captured.dice.v1",
        (),
        (roll,),
        {"value": result},
        builder.scope(match.start()),
    )
    builder.expression(
        "predicate.dice.matches_declaration.v1",
        (lower, upper, selection, die),
        (_span(match, "compare"),),
        {"declared": declaration, "event": event, "result": result},
        builder.scope(match.start()),
    )
    return True


def prepare_events(builder: Builder) -> str | None:
    """Claim the sealed counter, event and dice forms."""
    if builder.original.source_span.role != "body":
        return None
    for name, adapter in (
        ("H01", _ex_counters),
        ("R02", _damage),
        ("P04", _destroyed),
        ("D01", _dice_history),
        ("D02", _dice_declaration),
    ):
        if adapter(builder):
            return name
    return None
