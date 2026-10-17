"""Required vocabulary belongs to complete N3 shapes, including raw reminder anchors."""

import re
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.four_layer import FaceRevisionOwner, PrintingFaceOwner, Span
from sve_carddb.domains.translations.recognition.models import Candidate, Hint
from sve_carddb.domains.translations.recognition.provenance import merged

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_classification import (
        SourceReferences,
    )
    from sve_carddb.domains.translations.four_layer_normalizer import (
        SourceField,
        SourcePart,
    )

_WORDS = {
    "term:player.self": ("自分",),
    "term:player.opponent": ("相手",),
    "term:zone.deck": ("デッキ",),
    "term:zone.evolve_deck": ("エボルヴデッキ",),
    "term:zone.cemetery": ("墓場",),
    "term:zone.ex": ("EXエリア",),
    "term:object.card": ("カード",),
    "term:object.card_name": ("カード名",),
    "term:object.start_amulet": ("スタートアミュレット",),
    "term:rule.cost": ("コスト",),
    "term:rule.original_cost": ("元のコスト",),
    "term:rule.instead": ("代わりに",),
    "term:rule.ability": ("能力",),
    "term:rule.damage": ("ダメージ",),
    "term:stat.health": ("体力",),
    "term:event.evolve": ("進化時",),
    "term:action.banish": ("消滅させる", "消滅"),
    "term:ability.lesson": ("レッスン",),
    "term:ability.necrocharge": ("ネクロチャージ", "NC"),
    "term:ability.spell_chain": ("スペルチェイン", "SC"),
    "type/follower": ("フォロワー",),
    "type/leader": ("リーダー",),
}
_SELF_DECK = ("term:player.self", "term:zone.deck")
_CARD_COST = ("term:object.card", "term:rule.original_cost", "term:rule.cost")
_DIGIT = r"(?:N|[0-9０-９]+)"


def _family(part: SourcePart, field: SourceField) -> tuple[str, ...]:
    text = part.canonical_source
    if part.source_span.role == "reminder":
        return _reminder_family(text)
    if part.source_span.role != "body":
        return ()
    if field.source.field == "section":
        if text == r"『X』は自分のデッキにN枚まで入れることができる。":
            return _SELF_DECK
        if (
            text
            == "カード名に『X』を含まないカードは、メインデッキやエボルヴデッキにそれぞれN枚までしか入れられない。"
        ):
            return (
                "term:object.card_name",
                "term:object.card",
                "term:zone.deck",
                "term:zone.evolve_deck",
            )
    return _body_family(text)


def _reminder_family(text: str) -> tuple[str, ...]:
    if re.fullmatch(
        r"[（(]【スタートアミュレット】を持つカードはデッキに[1１]種しか入れられない[）)]",
        text,
    ):
        return "term:object.start_amulet", "term:object.card", "term:zone.deck"
    if re.fullmatch(
        r"[（(]【レッスン_"
        + _DIGIT
        + r"】とは、自分のEXエリアの『[^』]+』"
        + _DIGIT
        + r"枚を消滅させること[）)]",
        text,
    ):
        return (
            "term:ability.lesson",
            "term:player.self",
            "term:zone.ex",
            "term:action.banish",
        )
    if re.fullmatch(
        r"[（(]自分の墓場が"
        + _DIGIT
        + r"枚以上なら、【ネクロチャージ_"
        + _DIGIT
        + r"】の後の能力が働く。"
        + _DIGIT
        + r"枚以上なら、さらに【NC_"
        + _DIGIT
        + r"】の後の能力が働く[）)]",
        text,
    ):
        return "term:ability.necrocharge", "term:player.self", "term:zone.cemetery"
    return ()


def _body_family(text: str) -> tuple[str, ...]:
    if re.fullmatch(
        r"【進化時】自分の墓場にあるカードの元のコストの種類数がN種類以上なら、[^。]+。N種類以上なら、[^。]+。N種類以上なら、[^。]+。N種類以上なら、[^。]+。",
        text,
    ):
        return (
            "term:event.evolve",
            "term:player.self",
            "term:player.opponent",
            "term:zone.cemetery",
            "term:object.card",
            "term:rule.original_cost",
            "term:stat.health",
            "type/follower",
            "type/leader",
            "term:rule.damage",
        )
    if re.fullmatch(
        r"これをプレイする際、墓場の元のコストN(?:、N){9}のカードをN枚ずつ消滅:コストを-Nする。",
        text,
    ):
        return *_CARD_COST, "term:zone.cemetery", "term:action.banish"
    if re.fullmatch(
        r"これをプレイする際、【スペルチェイン_N】コストを-Nする。【(?:SC|スペルチェイン)_N】代わりに-Nする。【(?:SC|スペルチェイン)_N】代わりに-Nする。",
        text,
    ):
        return "term:ability.spell_chain", "term:rule.cost", "term:rule.instead"
    if (
        text
        == "これがいる限り、自分のフォロワーが受ける「相手の能力によるダメージ」を-Nする。"
    ):
        return (
            "term:player.self",
            "term:player.opponent",
            "type/follower",
            "term:rule.ability",
            "term:rule.damage",
        )
    return ()


def reference_candidate(
    part: SourcePart,
    candidate: Candidate,
    field: SourceField | None,
    refs: SourceReferences,
) -> Candidate:
    """Add fixed words only after recognizing an entire approved source construction."""
    if (
        field is None
        or field.source.field not in {"effect", "section"}
        or not isinstance(field.source.owner, (FaceRevisionOwner, PrintingFaceOwner))
    ):
        return candidate
    members = _family(part, field)
    specs = sorted(
        (
            (spelling, identifier)
            for identifier in members
            for spelling in _WORDS[identifier]
        ),
        key=lambda item: -len(item[0]),
    )
    occupied = [h.occurrence for h in candidate.slots]
    occupied.extend(
        Span(start=m.start(), end=m.end())
        for m in re.finditer(r"『[^』]*』", part.canonical_source)
    )
    hints = list(candidate.slots)
    issues = set(candidate.issues)
    for spelling, identifier in specs:
        for match in re.finditer(re.escape(spelling), part.canonical_source):
            span = Span(start=match.start(), end=match.end())
            if any(s.start < span.end and span.start < s.end for s in occupied):
                continue
            target = _target(identifier, spelling, refs)
            if target is None:
                issues.add("missing_n3_required_reference")
                continue
            positions = merged(
                tuple(s for u in part.units[span.start : span.end] for s in u.origins)
            )
            hints.append(
                Hint(
                    name=f"n3_reference_{len(hints)}",
                    occurrence=span,
                    source_segments=positions,
                    transformation="identity",
                    semantic_role="n3_filter_kind"
                    if identifier.startswith("type/")
                    else "term",
                    numeric_rule=None,
                    type="reference",
                    reference_kind="vocabulary"
                    if identifier.startswith("type/")
                    else "term",
                    value=None,
                    target=target,
                    issues=(),
                )
            )
            occupied.append(span)
    return candidate.model_copy(
        update={"slots": tuple(hints), "issues": tuple(sorted(issues))}
    )


def _target(
    identifier: str, spelling: str, refs: SourceReferences
) -> dict[str, JsonValue] | None:
    if identifier.startswith("type/"):
        resolution = refs.proposed_vocabulary("type", spelling)
        return (
            dict(resolution.target)
            if not resolution.issues and resolution.target is not None
            else None
        )
    category = (
        "ability"
        if identifier.startswith("term:ability.") or identifier == "term:event.evolve"
        else "rule_term"
    )
    members = refs.terms.get(spelling, ())
    if (identifier, category) in members:
        return {"kind": "term", "id": identifier}
    alias = {
        "NC": ("ネクロチャージ", "term:ability.necrocharge"),
        "SC": ("スペルチェイン", "term:ability.spell_chain"),
        "消滅": ("消滅させる", "term:action.banish"),
    }.get(spelling)
    if (
        alias is not None
        and alias[1] == identifier
        and (identifier, category) in refs.terms.get(alias[0], ())
    ):
        return {"kind": "term", "id": identifier}
    return None
