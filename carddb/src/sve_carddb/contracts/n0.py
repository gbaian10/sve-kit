"""Closed production leaf registry pinned by the single four-layer JP N0 version."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Frame, LeafSlot

VERSION = "four-layer-jp-v1"
SAFE_INTEGER = 9007199254740991
NAT_ROLES = frozenset(
    {
        "count",
        "damage_amount",
        "recovery_amount",
        "counter_amount",
        "repeat_count",
        "duration_count",
        "resource_amount",
        "cost_value",
        "stat_value",
        "threshold",
        "arithmetic_multiplier",
        "group_divisor",
        "stat_delta_magnitude",
        "cost_delta_magnitude",
        "resource_delta_magnitude",
        "damage_delta_magnitude",
        "counter_delta_magnitude",
    }
)
ORDINAL_ROLES = frozenset(
    {"choice_index", "card_index", "repeat_index", "turn_index", "deck_index"}
)
QUANTITY_ROLES = frozenset(
    {
        "selection_count",
        "choice_mode_count",
        "existence_count",
        "counter_amount",
        "repeat_count",
    }
)
REFERENCE_CODES = {
    ("Concept", "keyword"): "concept.keyword.v1",
    ("Concept", "ability"): "concept.ability.v1",
    ("Concept", "rule_term"): "concept.rule_term.v1",
    ("Concept", "trait"): "concept.trait.v1",
    ("Concept", "declared_trait"): "concept.trait.v1",
    ("Concept", "class_filter"): "vocabulary.class.v1",
    ("Concept", "declared_class"): "vocabulary.class.v1",
    **{
        ("CardName", role): "card_name.any.v1"
        for role in (
            "declared_name",
            "name_filter",
            "created_name",
            "name_reference",
        )
    },
    **{
        ("CardKind", role): "card_kind.any.v1"
        for role in (
            "counted_kind",
            "filter_kind",
            "declared_kind",
        )
    },
    ("LiteralLayout", "layout"): "layout.whitespace.v1",
}
SEMANTIC_CONSTRUCTIONS = {
    "metadata.name.v1": ("name", "none", 0),
    "metadata.label.v1": ("label", "none", 0),
    "metadata.layout.v1": ("layout", "none", 0),
    "card_keywords.v1": ("body", "card_field", 0),
    "evolve_entry.v1": ("body", "ability_body", 1),
    "feed_entry.v1": ("body", "ability_body", 1),
    "ride_entry.v1": ("body", "ability_body", 1),
    "evolve_feed_entries.v1": ("body", "ability_body", 2),
    "evolve_ride_entries.v1": ("body", "ability_body", 2),
    "token_header.v1": ("token_header", "card_field", 0),
    "pure_reminder.v1": ("reminder", "none", 0),
}


def verify_slot(slot: LeafSlot) -> None:
    """Generic future types cannot acquire production N0 identity accidentally."""
    if not slot.required or not slot.occurrences:
        raise ValueError("N0 leaves require explicit source occurrences")
    if slot.type in {"Nat", "Ordinal"}:
        roles = NAT_ROLES if slot.type == "Nat" else ORDINAL_ROLES
        minimum = (
            1
            if slot.type == "Ordinal"
            or slot.role in {"arithmetic_multiplier", "group_divisor"}
            else 0
        )
        if (
            slot.role not in roles
            or slot.domain.min != minimum
            or slot.domain.max != SAFE_INTEGER
        ):
            raise ValueError("Unregistered N0 numeric role or bounds")
        return
    code: str | None
    if slot.type == "QuantitySpec" and slot.role in QUANTITY_ROLES:
        code = f"quantity.{slot.role}.constant.v1"
    else:
        code = REFERENCE_CODES.get((slot.type, slot.role))
    if code is None or slot.domain.values != (code,):
        raise ValueError("Unregistered N0 type, role or named domain")


def verify_frame(frame: Frame) -> None:
    """N0 pins the whole registry, rather than independently versioning rule families."""
    if frame.source.normalizer_version != VERSION:
        return
    for slot in frame.leaf_schema.slots:
        verify_slot(slot)
    if frame.projection.imports or frame.projection.exports:
        raise ValueError("N0 cannot produce cross-frame ports")
    if frame.semantic_variant.state == "resolved":
        key = frame.semantic_variant.key
        descriptor = SEMANTIC_CONSTRUCTIONS.get(key) if key is not None else None
        if descriptor is None:
            raise ValueError("Unregistered N0 semantic construction")
        role, kind, abilities = descriptor
        projection = frame.projection
        if (
            frame.role != role
            or projection.projection_kind != kind
            or projection.discriminator != key
            or tuple((s.id, s.parent, s.kind) for s in projection.scopes)
            != tuple((f"ability_{i}", None, "ability") for i in range(abilities))
        ):
            raise ValueError("N0 projection differs from its registered construction")
