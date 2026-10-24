"""Resolve a complete printed Lastword without granting author admission."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    Domain,
    FaceRevisionOwner,
    GlossaryReference,
    LeafSchema,
    LeafSlot,
    Projection,
    Scope,
    Span,
)
from sve_carddb.domains.translations.four_layer_semantics import Classified

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField
    from sve_carddb.domains.translations.four_layer_semantics import CardContext

KEY = "damage_uniform_lastword_all_leaders.v1"
_SOURCE = re.compile(
    r"\{ラストワード\}リーダーすべてに(?P<count>0|[1-9][0-9]*)ダメージ。"
)
_SCHEMA = LeafSchema(
    format=2,
    slots=(
        LeafSlot(
            name="leaf_0",
            type="Concept",
            role="ability",
            domain=Domain(min=None, max=None, values=("concept.ability.v1",)),
            required=True,
            occurrences=(Span(start=1, end=7),),
        ),
        LeafSlot(
            name="leaf_1",
            type="Nat",
            role="damage_amount",
            domain=Domain(min=0, max=9007199254740991, values=()),
            required=True,
            occurrences=(Span(start=16, end=17),),
        ),
    ),
)


def prepare_damage(
    raw: str, prepared: DerivedField, context: CardContext | None
) -> DerivedField:
    """CR 12.5 and 5.14 fix the printed trigger and one all-leader damage batch."""
    source = prepared.field.source
    match = _SOURCE.fullmatch(raw)
    if match is None or context is None or context.source != source:
        return prepared
    if (
        not context.current
        or context.card_kind != "follower"
        or context.phase != "normal"
        or context.special_kinds != frozenset()
        or context.section_count != 0
    ):
        return prepared
    if (
        not isinstance(source.owner, FaceRevisionOwner)
        or source.field != "effect"
        or source.ordinal is not None
        or len(prepared.field.parts) != 1
    ):
        return prepared
    part = prepared.field.parts[0]
    found = prepared.recognized[0]
    if (
        part.line_ordinal != 0
        or part.source_span.role != "body"
        or part.source_span.segments != (Span(start=0, end=len(raw)),)
        or part.canonical_source != "{ラストワード}リーダーすべてにNダメージ。"
        or prepared.interfaces[0] is not None
    ):
        return prepared
    if (
        found.issues
        or found.schema != _SCHEMA
        or found.values.get("leaf_0")
        != GlossaryReference(kind="glossary", key="term:ability.last_words")
        or type(found.values.get("leaf_1")) is not int
        or found.values.get("leaf_1") != int(match["count"])
    ):
        return prepared
    if found.semantics is not None:
        raise ValueError("ambiguous_semantic_variant")
    classified = replace(
        found,
        semantics=Classified(
            KEY,
            Projection(
                projection_kind="ability_body",
                discriminator=KEY,
                scopes=(Scope(id="ability_0", kind="ability", parent=None),),
                imports=(),
                exports=(),
            ),
        ),
    )
    return replace(
        prepared, derived=replace(prepared.derived, recognized=(classified,))
    )
