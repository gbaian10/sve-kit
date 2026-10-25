"""Admit three closed source constructions with their exact operand contracts."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    Constant,
    Domain,
    FaceRevisionOwner,
    GlossaryReference,
    LeafSchema,
    LeafSlot,
    Projection,
    QuantitySpec,
    Scope,
    Span,
)
from sve_carddb.domains.translations.four_layer_semantics import Classified

if TYPE_CHECKING:
    from sve_carddb.contracts.source_binding import TypedValue
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField
    from sve_carddb.domains.translations.four_layer_semantics import CardContext

_MAX_NAT = 9007199254740991
_MOVE_PARTS = 3
_MOVE_INDEX = 2
SEARCH = "search_lastword_self_deck_named_hand.v1"
NUMERIC = "numeric_fanfare_other_allied_followers_add_1_1.v1"
MOVE = "move_self_to_owner_cemetery_activated_pp.v1"
_SEARCH = re.compile(
    r"\{ラストワード\}自分のデッキから『式神の使役』(?P<n>0|[1-9][0-9]{0,15})枚を探し、手札に加える。"
)
_NUMERIC = "{ファンファーレ}自分の他のフォロワーすべては{攻撃力}+1/{体力}+1する。"
_MOVE = "{起動}{コスト1}：これを墓場に置く。"
_SIBLING = "これが場を離れたとき、1枚引く。自分の手札1枚を捨てる。"
_CANONICAL = {
    SEARCH: "{ラストワード}{leaf_1}の{leaf_2}から『X』N枚を探し、{leaf_5}に加える。",
    NUMERIC: "{ファンファーレ}自分の他のフォロワーすべては{攻撃力}+N/{体力}+Nする。",
    MOVE: "{起動}{コストN}:これを{leaf_2}に置く。",
}


def _slot(
    name: str, kind: str, role: str, domain: str, start: int, end: int
) -> LeafSlot:
    return LeafSlot.model_validate(
        {
            "name": name,
            "type": kind,
            "role": role,
            "domain": Domain(min=0, max=_MAX_NAT, values=())
            if kind == "Nat"
            else Domain(min=None, max=None, values=(domain,)),
            "required": start != end,
            "occurrences": () if start == end else (Span(start=start, end=end),),
        }
    )


_SEARCH_SCHEMA = LeafSchema(
    format=2,
    slots=(
        _slot("leaf_0", "Concept", "ability", "concept.ability.v1", 1, 7),
        _slot("leaf_1", "Player", "source_owner", "player.relative.v1", 8, 16),
        _slot("leaf_2", "ZoneSet", "source_zone", "zone.single.v1", 17, 25),
        _slot("leaf_3", "CardName", "name_reference", "card_name.any.v1", 28, 29),
        _slot(
            "leaf_4",
            "QuantitySpec",
            "selection_count",
            "quantity.selection_count.constant.v1",
            30,
            31,
        ),
        _slot("leaf_5", "ZoneSet", "destination_zone", "zone.hand.v1", 36, 44),
        _slot("leaf_6", "Player", "destination_owner", "player.self.v1", 0, 0),
    ),
)
_NUMERIC_SCHEMA = LeafSchema(
    format=2,
    slots=(
        _slot("leaf_0", "Concept", "ability", "concept.ability.v1", 1, 8),
        _slot("leaf_1", "Concept", "rule_term", "concept.rule_term.v1", 24, 27),
        _slot("leaf_2", "Nat", "stat_delta_magnitude", "", 29, 30),
        _slot("leaf_3", "Concept", "rule_term", "concept.rule_term.v1", 32, 34),
        _slot("leaf_4", "Nat", "stat_delta_magnitude", "", 36, 37),
    ),
)
_MOVE_SCHEMA = LeafSchema(
    format=2,
    slots=(
        _slot("leaf_0", "Concept", "ability", "concept.ability.v1", 1, 3),
        _slot("leaf_1", "Nat", "cost_value", "", 8, 9),
        _slot("leaf_2", "ZoneSet", "destination_zone", "zone.single.v1", 14, 22),
    ),
)
_SIBLING_SCHEMA = LeafSchema(
    format=2,
    slots=(
        _slot("leaf_0", "Nat", "count", "", 11, 12),
        _slot("leaf_1", "Nat", "count", "", 21, 22),
    ),
)


def _context_matches(
    prepared: DerivedField, context: CardContext | None, *, token: bool
) -> bool:
    source = prepared.field.source
    return (
        context is not None
        and context.source == source
        and context.current
        and context.phase == "normal"
        and context.card_kind == ("amulet" if token else "follower")
        and context.special_kinds == (frozenset({"token"}) if token else frozenset())
        and context.section_count == 0
        and isinstance(source.owner, FaceRevisionOwner)
        and source.field == "effect"
        and source.ordinal is None
    )


def _resolve(
    prepared: DerivedField,
    index: int,
    key: str,
    schema: LeafSchema,
    values: dict[str, TypedValue],
) -> DerivedField:
    found = prepared.recognized[index]
    if prepared.interfaces[index] is not None:
        return prepared
    if (
        found.issues
        or prepared.field.parts[index].canonical_source != _CANONICAL[key]
        or found.schema != schema
        or found.values != values
        or any(
            type(found.values.get(slot.name)) is not int
            for slot in schema.slots
            if slot.type == "Nat"
        )
    ):
        return prepared
    if found.semantics is not None:
        raise ValueError("ambiguous_semantic_variant")
    recognized = list(prepared.recognized)
    recognized[index] = replace(
        found,
        semantics=Classified(
            key,
            Projection(
                projection_kind="ability_body",
                discriminator=key,
                scopes=(Scope(id="ability_0", kind="ability", parent=None),),
                imports=(),
                exports=(),
            ),
        ),
    )
    return replace(
        prepared, derived=replace(prepared.derived, recognized=tuple(recognized))
    )


def prepare_fixed_actions(
    raw: str, prepared: DerivedField, context: CardContext | None
) -> DerivedField:
    """A complete field fixes holder, head and siblings before an action is admitted."""
    if (
        _context_matches(prepared, context, token=False)
        and len(prepared.field.parts) == 1
    ):
        part = prepared.field.parts[0]
        if (
            part.line_ordinal != 0
            or part.source_span.role != "body"
            or part.source_span.segments != (Span(start=0, end=len(raw)),)
        ):
            return prepared
        if (match := _SEARCH.fullmatch(raw)) and int(match["n"]) <= _MAX_NAT:
            return _resolve(
                prepared,
                0,
                SEARCH,
                _SEARCH_SCHEMA,
                {
                    "leaf_0": GlossaryReference(
                        kind="glossary", key="term:ability.last_words"
                    ),
                    "leaf_1": "self",
                    "leaf_2": ("deck",),
                    "leaf_3": CardNameReference(
                        kind="card_name", term_id="term:name.shikigami_employment"
                    ),
                    "leaf_4": QuantitySpec(
                        mode="exact",
                        expr=Constant(kind="constant", value=int(match["n"])),
                    ),
                    "leaf_5": ("hand",),
                    "leaf_6": "self",
                },
            )
        if raw == _NUMERIC:
            return _resolve(
                prepared,
                0,
                NUMERIC,
                _NUMERIC_SCHEMA,
                {
                    "leaf_0": GlossaryReference(
                        kind="glossary", key="term:ability.fanfare"
                    ),
                    "leaf_1": GlossaryReference(
                        kind="glossary", key="term:stat.attack"
                    ),
                    "leaf_2": 1,
                    "leaf_3": GlossaryReference(
                        kind="glossary", key="term:stat.health"
                    ),
                    "leaf_4": 1,
                },
            )
    return _prepare_move(raw, prepared, context)


def _move_siblings_match(prepared: DerivedField) -> bool:
    sibling, layout, _ = prepared.recognized
    if (
        sibling.issues
        or sibling.schema != _SIBLING_SCHEMA
        or prepared.field.parts[0].canonical_source
        != "これが場を離れたとき、N枚引く。自分の手札N枚を捨てる。"
        or sibling.values != {"leaf_0": 1, "leaf_1": 1}
        or any(prepared.interfaces)
    ):
        return False
    return (
        not layout.issues
        and layout.values == {"leaf_0": "\n"}
        and prepared.field.parts[1].canonical_source == "W"
        and layout.semantics is not None
        and layout.semantics.key == "metadata.layout.v1"
    )


def _prepare_move(
    raw: str, prepared: DerivedField, context: CardContext | None
) -> DerivedField:
    if (
        not _context_matches(prepared, context, token=True)
        or raw != _SIBLING + "\n" + _MOVE
        or len(prepared.field.parts) != _MOVE_PARTS
    ):
        return prepared
    for index, role, start, end in (
        (0, "body", 0, len(_SIBLING)),
        (1, "layout", len(_SIBLING), len(_SIBLING) + 1),
        (2, "body", len(_SIBLING) + 1, len(raw)),
    ):
        part = prepared.field.parts[index]
        if (
            part.line_ordinal != (1 if index == _MOVE_INDEX else 0)
            or part.source_span.role != role
            or part.source_span.segments != (Span(start=start, end=end),)
        ):
            return prepared
    # The unresolved leave-field ability remains an independent obligation of the token.
    if not _move_siblings_match(prepared):
        return prepared
    return _resolve(
        prepared,
        2,
        MOVE,
        _MOVE_SCHEMA,
        {
            "leaf_0": GlossaryReference(kind="glossary", key="term:ability.activation"),
            "leaf_1": 1,
            "leaf_2": ("graveyard",),
        },
    )
