"""Resolve the complete Advance Fanfare while retaining its nonbody siblings."""

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

KEY = "destroy_fanfare_opponent_all_field_followers.v1"
_BODY = "{ファンファーレ}相手の場のフォロワーすべてを破壊する。"
_RAW = "【守護】【オーラ】\n" + _BODY
_PART_COUNT = 3
_SCHEMA = LeafSchema(
    format=2,
    slots=(
        LeafSlot(
            name="leaf_0",
            type="Concept",
            role="ability",
            domain=Domain(min=None, max=None, values=("concept.ability.v1",)),
            required=True,
            occurrences=(Span(start=1, end=8),),
        ),
    ),
)


def _siblings_match(prepared: DerivedField) -> bool:
    for ordinal, role, start, end, text, key, kind in (
        (0, "body", 0, 9, "【守護】【オーラ】", "card_keywords.v2", "card_field"),
        (1, "layout", 9, 10, "W", "metadata.layout.v1", "none"),
    ):
        part = prepared.field.parts[ordinal]
        found = prepared.recognized[ordinal]
        if (
            part.line_ordinal != 0
            or part.source_span.role != role
            or part.source_span.segments != (Span(start=start, end=end),)
            or part.canonical_source != text
        ):
            return False
        if found.issues or found.semantics is None:
            return False
        projection = found.semantics.projection
        if (
            found.semantics.key != key
            or projection.projection_kind != kind
            or projection.discriminator != key
            or (projection.scopes, projection.imports, projection.exports)
            != ((), (), ())
        ):
            return False
    return prepared.recognized[0].values == {
        "leaf_0": GlossaryReference(kind="glossary", key="term:keyword.ward"),
        "leaf_1": GlossaryReference(kind="glossary", key="term:keyword.aura"),
    } and prepared.recognized[1].values == {"leaf_0": "\n"}


def prepare_destroy(
    raw: str, prepared: DerivedField, context: CardContext | None
) -> DerivedField:
    """The admitted source includes Ward/Aura and one complete, untargeted Fanfare."""
    source = prepared.field.source
    if raw != _RAW or context is None or context.source != source:
        return prepared
    if (
        not context.current
        or context.card_kind != "follower"
        or context.phase != "advance"
        or context.special_kinds != frozenset({"advance"})
        or context.section_count != 0
    ):
        return prepared
    if (
        not isinstance(source.owner, FaceRevisionOwner)
        or source.field != "effect"
        or source.ordinal is not None
        or len(prepared.field.parts) != _PART_COUNT
    ):
        return prepared
    part = prepared.field.parts[2]
    found = prepared.recognized[2]
    if (
        part.line_ordinal != 1
        or part.source_span.role != "body"
        or part.source_span.segments != (Span(start=10, end=38),)
        or part.canonical_source != _BODY
        or any(prepared.interfaces)
    ):
        return prepared
    if (
        found.issues
        or found.schema != _SCHEMA
        or found.values
        != {"leaf_0": GlossaryReference(kind="glossary", key="term:ability.fanfare")}
        or not _siblings_match(prepared)
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
        prepared,
        derived=replace(
            prepared.derived,
            recognized=(*prepared.recognized[:2], classified),
        ),
    )
