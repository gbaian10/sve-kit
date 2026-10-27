"""Finite aggregate source contracts, isolated from B11 and executable DSL rules."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING, cast

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    GlossaryReference,
    VocabularyReference,
)
from sve_carddb.domains.translations.four_layer_context import Descriptor, Parameter

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import LeafType
    from sve_carddb.contracts.source_binding import TypedValue


def operand(name: str) -> Parameter:
    """Keep aggregate operands in their own roles without widening old read domains."""
    types = {
        "owner": ("Player", "player.relative.v1"),
        "zone": ("ZoneSet", "zone.single.v1"),
        "kind": ("CardKind", "card_kind.selection.v1"),
        "trait": ("Concept", "concept.trait.v1"),
        "card": ("Concept", "concept.rule_term.v1"),
        "token": ("TokenStatus", "token_status.token.v1"),
        "name": ("CardName", "card_name.any.v1"),
        "class": ("Concept", "vocabulary.class.v1"),
        "cost": ("Nat", ""),
        "threshold": ("Nat", ""),
        "offset": ("Nat", ""),
    }
    base = name.removesuffix("_left").removesuffix("_right")
    type_, domain = types[base]
    return Parameter(name, cast("LeafType", type_), "aggregate_" + base, domain)


# Each noun signature preserves printed alternatives/conjunctions in its descriptor.
NOUNS = {
    "card": ("card",),
    "kind": ("kind",),
    "trait_card": ("trait", "card"),
    "trait_kind": ("trait", "kind"),
    "trait_token": ("trait", "token"),
    "trait_token_kind": ("trait", "token", "kind"),
    "traits_kind": ("trait_left", "trait_right", "kind"),
    "alternatives": ("trait_left", "kind_left", "trait_right", "kind_right"),
    "class_cost_le": ("cost", "class", "kind"),
    "cost_eq_kind": ("cost", "kind"),
    "exclude_name": ("trait", "card"),
    "distinct_names": ("trait_left", "trait_right", "kind"),
    "name": ("name",),
    "filtered_trait_card": ("trait", "card"),
    "filtered_kind": ("kind",),
    "distinct_filtered_kind": ("kind",),
}


def count_id(shape: str) -> str:
    """Identify a closed selector count signature."""
    return f"aggregate.count.{shape}.v1"


def sum_id(origin: str, shape: str, property_: str) -> str:
    """Keep source position and summed property in the structure."""
    return f"aggregate.sum.{origin}.{shape}.{property_}.v1"


_COUNT_SHAPES = (
    "card",
    "trait_card",
    "kind",
    "trait_token",
    "trait_token_kind",
    "name",
)
_SUM_SHAPES = (
    ("looked", "trait_kind", "original_cost"),
    ("looked", "alternatives", "original_cost"),
    ("looked", "class_cost_le", "original_cost"),
    ("deck", "trait_kind", "original_cost"),
    ("deck", "alternatives", "original_cost"),
    ("deck", "exclude_name", "original_cost"),
    ("deck", "distinct_names", "original_cost"),
    ("graveyard", "trait_kind", "original_cost"),
    ("graveyard", "trait_card", "original_cost"),
    ("graveyard", "traits_kind", "original_cost"),
    ("payment", "kind", "original_cost"),
    ("battlefield", "trait_card", "original_cost"),
    ("battlefield", "cost_eq_kind", "attack"),
)


def _descriptor(  # ruff: ignore[too-many-arguments] -- each closed descriptor declares type, dimension and source bridge
    id_: str,
    domain: str,
    role: str,
    measure: str,
    parameters: tuple[Parameter, ...],
    *,
    predicate: bool = False,
    family: str = "A8",
    bridge: tuple[str, ...] = ("X.aggregate",),
) -> Descriptor:
    return Descriptor(
        id_,
        domain,
        role,
        "Bool" if predicate else "Nat",
        measure,
        parameters,
        (),
        family,
        bridge,
    )


def _quantity(name: str, role: str, domain: str) -> Parameter:
    return Parameter(name, "QuantityExpr", role, domain)


_COUNT = tuple(
    _descriptor(
        count_id(shape),
        "quantity.aggregate.source_count.v1",
        "aggregate_count",
        "card_count",
        tuple(operand(p) for p in ("owner", "zone", *NOUNS[shape])),
    )
    for shape in _COUNT_SHAPES
)
_SUM = tuple(
    _descriptor(
        sum_id(origin, shape, prop),
        "quantity.aggregate.property_sum.v1",
        "property_sum_value",
        "attack" if prop == "attack" else "cost",
        tuple(
            operand(p)
            for p in (
                *(
                    ("owner", "zone")
                    if origin in {"deck", "graveyard", "battlefield"}
                    else ()
                ),
                *NOUNS[shape],
            )
        ),
        bridge=("X.aggregate.Sum", "V.stats" if prop == "attack" else "V.cost"),
    )
    for origin, shape, prop in _SUM_SHAPES
)
_COMPOSED_SUM = tuple(
    _descriptor(
        sum_id(origin, shape, "original_cost"),
        "quantity.aggregate.property_sum.v1",
        "property_sum_value",
        "cost",
        (
            *(operand(p) for p in ("owner", "zone", *NOUNS[shape])),
            *(
                Parameter(name, "PredicateExpr", role, domain)
                for name, role, domain in predicates
            ),
        ),
        bridge=("X.aggregate.Sum", "V.cost", "S.filter"),
    )
    for origin, shape, predicates in (
        (
            "deck",
            "filtered_trait_card",
            (("name", "name_relation", "predicate.name.relative.v1"),),
        ),
        (
            "graveyard",
            "filtered_kind",
            (("traits", "trait_conjunction", "predicate.traits.all2.v1"),),
        ),
        (
            "deck",
            "distinct_filtered_kind",
            (
                ("traits", "trait_conjunction", "predicate.traits.all2.v1"),
                (
                    "distinct",
                    "selection_name_constraint",
                    "predicate.selection.distinct_name.v1",
                ),
            ),
        ),
    )
)
_TOTAL = tuple(
    _descriptor(
        f"aggregate.total.{link}{suffix}.v1",
        "quantity.aggregate.source_total.v1",
        "aggregate_read_value",
        "card_count",
        (
            _quantity("left", "aggregate_count", "quantity.aggregate.source_count.v1"),
            _quantity("right", "aggregate_count", "quantity.aggregate.source_count.v1"),
            *((operand("offset"),) if suffix else ()),
        ),
        bridge=("X.aggregate",) if link == "ya" else ("X.aggregate", "X.math.Add"),
    )
    for link, suffix in (("to", ""), ("ya", ""), ("adjacent", ".offset"))
)
_BOUND = tuple(
    _descriptor(
        f"aggregate.bound.{position}.{comparison}.v1",
        f"predicate.aggregate.{position}_bound.v1",
        f"aggregate_{position}_constraint",
        "truth",
        (
            _quantity(
                "value", "property_sum_value", "quantity.aggregate.property_sum.v1"
            ),
            operand("threshold"),
        ),
        predicate=True,
        bridge=(
            "S.constraint.aggregate_bounds"
            if position != "condition"
            else "X.predicate",
        ),
    )
    for position in ("selection", "payment", "condition")
    for comparison in ("ge", "le", "eq")
)
_NAMES = (
    CardNameReference(kind="card_name", term_id="term:name.blaster_blade"),
    CardNameReference(kind="card_name", term_id="term:name.blaster_dark"),
)


def _pair_parameter(name: str, value: object) -> Parameter:
    return replace(operand(name), values=(cast("TypedValue", value),))


_PAIR = tuple(
    _descriptor(
        f"aggregate.pair.{shape}.v1",
        "quantity.group.disjoint_pair.v1",
        "pair_count",
        "pair_count",
        tuple(
            _pair_parameter(name + "_" + side, value)
            for side, ref, kind in (
                ("left", _NAMES[0], "follower"),
                ("right", _NAMES[1], "spell"),
            )
            for name, value in (
                ("owner", "self"),
                ("zone", ("graveyard",)),
                *(
                    (("name", ref),)
                    if shape == "named"
                    else (
                        (
                            "trait",
                            GlossaryReference(kind="glossary", key="term:trait.mage"),
                        ),
                        (
                            "kind",
                            VocabularyReference(kind="vocabulary", key=("type", kind)),
                        ),
                    )
                ),
            )
        ),
        family="A10",
        bridge=("X.aggregate.Count", "X.math.Min", "S.constraint.disjoint"),
    )
    for shape in ("named", "kind")
)
DESCRIPTORS = (*_COUNT, *_SUM, *_COMPOSED_SUM, *_TOTAL, *_BOUND, *_PAIR)


@dataclass(frozen=True)
class Lowering:
    descriptor: str
    operands: tuple[str, ...]
    unresolved: tuple[str, ...]
    pair_policy: str | None = None
    executable: bool = False


def lowering(descriptor: str, operands: tuple[str, ...]) -> Lowering:
    """Source identity stays fixed when future execution evidence settles deduplication."""
    if descriptor not in {d.id for d in DESCRIPTORS}:
        raise ValueError("Unknown aggregate source descriptor")
    reasons: tuple[str, ...] = ("typed_aggregate_lowering",)
    if descriptor == "aggregate.total.ya.v1":
        reasons = ("overlapping_total",)
    elif ".exclude_name." in descriptor:
        reasons = ("same_name_selector", "typed_aggregate_lowering")
    elif ".distinct_names." in descriptor:
        reasons = ("distinct_name_selector", "typed_aggregate_lowering")
    return Lowering(
        descriptor,
        operands,
        reasons,
        "min(left_count,right_count);each_card_at_most_once;disjoint"
        if descriptor.startswith("aggregate.pair.")
        else None,
    )
