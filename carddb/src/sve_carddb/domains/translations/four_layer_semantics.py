"""Finite N0 whole-part semantics keep lexical recognition separate from resolution."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    FaceRevisionOwner,
    PrintingFaceOwner,
    Projection,
    Scope,
)
from sve_carddb.domains.translations.source_inventory.normalizer import TOKEN_HEADER

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.contracts.four_layer import LeafSchema
    from sve_carddb.contracts.source_binding import SourceDescriptor
    from sve_carddb.domains.translations.four_layer_classification import Term
    from sve_carddb.domains.translations.four_layer_normalizer import (
        SourceField,
        SourcePart,
    )


@dataclass(frozen=True)
class CardContext:
    source: SourceDescriptor
    card_id: str
    face_id: str
    phase: str
    card_kind: str
    evolved_face_id: str | None


@dataclass(frozen=True)
class Classified:
    key: str
    projection: Projection


def _projection(key: str, kind: str, abilities: int = 0) -> Classified:
    return Classified(
        key,
        Projection.model_validate(
            {
                "projection_kind": kind,
                "discriminator": key,
                "scopes": tuple(
                    Scope(id=f"ability_{index}", parent=None, kind="ability")
                    for index in range(abilities)
                ),
                "imports": (),
                "exports": (),
            }
        ),
    )


_ENTRIES = {
    "evolve": re.compile(r"\{進化\}\{コストN\}:これは進化する。"),
    "feed": re.compile(r"\{食事\}\{コストN\}:これは出走する。"),
    "ride": re.compile(r"\{憑依\}\{コストN\}:これはドライブを持つ。"),
}
_KEYWORDS = re.compile(r"【([^【】]+)】(?:[ 、]*)")
_REMINDERS = {
    "疾走": re.compile(r"（(?:これは)?プレイしたターンから攻撃できる。）"),
    "突進": re.compile(r"（(?:これは)?プレイしたターンからフォロワーに攻撃できる。）"),
}


def classify_semantics(
    source: SourceDescriptor,
    part: SourcePart,
    schema: LeafSchema,
    terms: Mapping[str, Term],
    context: CardContext | None,
    field: SourceField | None,
) -> Classified | None:
    """Only complete registered constructions authorize a reusable semantic key."""
    role = part.source_span.role
    if role in {"name", "label", "layout"}:
        return _projection(f"metadata.{role}.v1", "none")
    if not isinstance(source.owner, (FaceRevisionOwner, PrintingFaceOwner)):
        return None
    if role == "token_header":
        return _header(source, part, schema)
    if role == "reminder":
        return _reminder(source, part, terms, field)
    if role != "body" or source.field not in {"effect", "section"}:
        return None
    return _body(source, part, schema, terms, context)


def _body(
    source: SourceDescriptor,
    part: SourcePart,
    schema: LeafSchema,
    terms: Mapping[str, Term],
    context: CardContext | None,
) -> Classified | None:
    found = []
    if _keywords(part.canonical_source, schema, terms):
        found.append(_projection("card_keywords.v1", "card_field"))
    if context is not None:
        if context.source != source:
            raise ValueError("Semantic context belongs to another exact owner field")
        if result := _entry(part.canonical_source, schema, context):
            found.append(result)
    if len(found) > 1:
        raise ValueError("ambiguous_semantic_variant")
    return found[0] if found else None


def _reminder(
    source: SourceDescriptor,
    part: SourcePart,
    terms: Mapping[str, Term],
    field: SourceField | None,
) -> Classified | None:
    if field is None or field.source != source or part not in field.parts:
        return None
    anchor = part.source_span.anchor
    if anchor is None or anchor >= len(field.parts):
        return None
    body = field.parts[anchor]
    if body.source_span.role != "body" or body.line_ordinal != part.line_ordinal:
        return None
    for keyword, grammar in _REMINDERS.items():
        registered = tuple(
            term
            for term in terms.values()
            if term.category == "keyword" and term.source_ja == keyword
        )
        if (
            len(registered) == 1
            and body.canonical_source == "【" + keyword + "】"
            and grammar.fullmatch(part.canonical_source)
        ):
            return _projection("pure_reminder.v1", "none")
    return None


def _keywords(text: str, schema: LeafSchema, terms: Mapping[str, Term]) -> bool:
    matches = tuple(_KEYWORDS.finditer(text))
    if not matches or "".join(m[0] for m in matches) != text:
        return False
    if any(
        sum(
            term.category == "keyword" and term.source_ja == m[1]
            for term in terms.values()
        )
        != 1
        for m in matches
    ):
        return False
    # N0 preserves these fixed keyword spellings without adding lexical leaves.
    return not schema.slots


def _header(
    source: SourceDescriptor, part: SourcePart, schema: LeafSchema
) -> Classified | None:
    if (
        source.field != "section"
        or len(part.source_span.segments) != 1
        or part.source_span.segments[0].start != 0
        or TOKEN_HEADER.fullmatch(part.canonical_source) is None
    ):
        return None
    roles = {slot.role for slot in schema.slots}
    if not {"declared_name", "declared_class", "declared_kind"} <= roles:
        return None
    return _projection("token_header.v1", "card_field")


def _entry(text: str, schema: LeafSchema, context: CardContext) -> Classified | None:
    if context.phase != "normal" or context.card_kind != "follower":
        return None
    costs = tuple(s for s in schema.slots if s.type == "Nat" and s.role == "cost_value")
    if not costs or any(s.role not in {"cost_value", "ability"} for s in schema.slots):
        return None
    if context.evolved_face_id is None:
        return None
    for family, pattern in _ENTRIES.items():
        if pattern.fullmatch(text) and len(costs) == 1:
            return _projection(f"{family}_entry.v1", "ability_body", 1)
    for second in ("feed", "ride"):
        pattern = re.compile(
            _ENTRIES["evolve"].pattern + r" *" + _ENTRIES[second].pattern
        )
        if pattern.fullmatch(text) and sum(len(s.occurrences) for s in costs) == len(
            ("evolve", second)
        ):
            return _projection(f"evolve_{second}_entries.v1", "ability_body", 2)
    return None
