"""Required vocabulary belongs to complete N3 shapes, including raw reminder anchors."""

import re
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.four_layer import FaceRevisionOwner, PrintingFaceOwner, Span
from sve_carddb.domains.translations.four_layer_n3_shapes import SHAPES
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


def _family(part: SourcePart, field: SourceField) -> tuple[str, ...]:
    for shape in SHAPES.values():
        if (
            part.source_span.role == shape.role
            and (not shape.section_only or field.source.field == "section")
            and shape.match(part.canonical_source)
        ):
            return shape.words
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
