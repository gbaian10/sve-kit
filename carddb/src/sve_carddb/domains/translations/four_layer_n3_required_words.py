"""Required words are collected only within an already sealed #749 construction."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import Span, VocabularyReference
from sve_carddb.domains.translations.four_layer_event_preparation import origins
from sve_carddb.domains.translations.four_layer_n3_shapes import EX_COUNTER_SUBJECT

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_n3_sources import Builder

_WORDS = {
    "counter": ("カウンター", "rule_term", "term:object.counter", "rule_term"),
    "card": ("カード", "rule_term", "term:object.card", "rule_term"),
    "die": ("サイコロ", "rule_term", "term:object.die", "rule_term"),
    "stack": ("スタック", "ability", "term:ability.stack", "ability"),
    "fusion": ("融合", "ability", "term:ability.fusion", "ability"),
    "evolve": ("進化時", "ability", "term:event.evolve", "ability"),
    "ability": ("能力", "rule_term", "term:rule.ability", "rule_term"),
}
_REQUIRED = {
    "H01": ("counter", "card", "evolve"),
    "H02": ("counter",),
    "H03": ("stack", "counter"),
    "H04": ("stack",),
    "D01": ("die",),
    "D02": ("die", "evolve"),
    "X01": ("fusion", "counter", "card"),
    "A01": ("evolve", "ability"),
    "P04": ("evolve",),
}


def _occupied(builder: Builder, span: Span) -> bool:
    raw = origins(builder.original, span)
    return any(
        a.start < b.end and b.start < a.end
        for item in builder.additions
        for group in item.positions
        for s in group
        for a in origins(builder.original, s)
        for b in raw
    ) or any(
        a.start < b.end and b.start < a.end
        for o in builder.found.occurrences
        for a in o.raw_spans
        for b in raw
    )


def required_words(builder: Builder, family: str) -> None:
    """Required source references cannot disappear behind literal target wording."""
    missing = False
    quotes = tuple(
        Span(start=m.start(), end=m.end())
        for m in re.finditer(r"『[^』]+』", builder.text)
    )
    for key in _REQUIRED.get(family, ()):
        word, category, identifier, role = _WORDS[key]
        for match in re.finditer(re.escape(word), builder.text):
            span = Span(start=match.start(), end=match.end())
            if any(
                s.start <= span.start < span.end <= s.end for s in quotes
            ) or _occupied(builder, span):
                continue
            missing |= builder.term(span, category, identifier, role) is None
    if family == "H01":
        _witness_subject(builder)
    if family == "P04":
        for match in re.finditer(r"フォロワー", builder.text):
            span = Span(start=match.start(), end=match.end())
            if not _occupied(builder, span):
                builder.scalar(
                    "CardKind",
                    "counted_kind",
                    "card_kind.selection.v1",
                    VocabularyReference(kind="vocabulary", key=("type", "follower")),
                    (span,),
                )
    if missing:
        builder.found = replace(
            builder.found,
            issues=tuple(
                sorted({*builder.found.issues, "missing_n3_required_reference"})
            ),
        )


def _witness_subject(builder: Builder) -> None:
    match = re.search(EX_COUNTER_SUBJECT, builder.text)
    if match is None:
        return
    owner = builder.scalar(
        "Player",
        "read_owner",
        "player.relative.v1",
        "self",
        (Span(start=match.start("owner"), end=match.end("owner")),),
    )
    zone = builder.scalar(
        "ZoneSet",
        "read_zone",
        "zone.single.v1",
        ("ex",),
        (Span(start=match.start("zone"), end=match.end("zone")),),
    )
    names = [owner, zone]
    names.extend(
        item.slot.name
        for item in builder.additions
        if item.slot.role == "count"
        or getattr(item.value, "key", None) == "term:object.card"
    )
    for i, port in enumerate(builder.ports):
        if port.source_role == "highest_satisfied_holder" and port.evidence is not None:
            builder.ports[i] = port.model_copy(
                update={
                    "evidence": port.evidence.model_copy(
                        update={"source_leaves": tuple(names)}
                    )
                }
            )
