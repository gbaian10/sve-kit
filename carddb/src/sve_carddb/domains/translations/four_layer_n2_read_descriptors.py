"""A1/A4 own their closed source signatures; protected B11 domains stay unchanged."""

from typing import TYPE_CHECKING, cast

from sve_carddb.domains.translations.four_layer_context import (
    DEFAULT_REGISTRY,
    Descriptor,
    ImportSignature,
    Parameter,
)

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import CaptureTime

PARAMETERS = {
    "value": Parameter("value", "QuantityExpr", "read_value", ""),
    "owner": Parameter("owner", "Player", "read_owner", "player.relative.v1"),
    "zone": Parameter("zone", "ZoneSet", "read_zone", "zone.single.v1"),
    "kind": Parameter("kind", "CardKind", "read_kind", "card_kind.selection.v1"),
    "trait": Parameter("trait", "Concept", "read_trait", "concept.trait.v1"),
    "class": Parameter("class", "Concept", "read_class", "vocabulary.class.v1"),
    "name": Parameter("name", "CardName", "read_name", "card_name.any.v1"),
    "keyword": Parameter("keyword", "Concept", "read_keyword", "concept.keyword.v1"),
    "card": Parameter("card", "Concept", "read_card", "concept.rule_term.v1"),
    "token_status": Parameter(
        "token_status", "TokenStatus", "read_token_status", "token_status.token.v1"
    ),
    "counter": Parameter("counter", "Concept", "read_counter", "concept.rule_term.v1"),
    "offset": Parameter("offset", "Nat", "arithmetic_offset", ""),
    "cost": Parameter("cost", "Nat", "read_original_cost", ""),
}


def _read(
    identifier: str,
    measure: str,
    parameters: tuple[str, ...] = (),
    imports: tuple[ImportSignature, ...] = (),
    bridge: tuple[str, ...] = ("V.zone-count", "S.filter"),
) -> Descriptor:
    return Descriptor(
        identifier,
        "quantity.read.context.v1",
        "read_value",
        "Nat",
        measure,
        tuple(PARAMETERS[p] for p in parameters),
        imports,
        "A1",
        bridge,
        scalable=True,
    )


_FACEUP = tuple(
    _read(
        "read.faceup_evolve." + suffix + ".v1",
        "card_count",
        parameters,
        bridge=("V.zone-count", "V.state", "S.filter"),
    )
    for suffix, parameters in (
        ("all", ("owner", "zone")),
        ("trait", ("owner", "zone", "trait")),
        ("placed", ("owner", "zone")),
    )
)
_OBJECTS = tuple(
    _read(
        f"read.{source}.{property_}.v1",
        "attack"
        if property_ == "attack"
        else "health"
        if property_ == "health"
        else "cost",
        (),
        (
            ImportSignature(
                "subject", "ObjectSet", cast("CaptureTime", capture), "one", origin
            ),
        ),
        ("V.stats",) if property_ in {"attack", "health"} else ("V.cost",),
    )
    for source, capture, origin, properties in (
        (
            "selected",
            "selection",
            "selection",
            ("attack", "health", "original_cost", "effective_cost"),
        ),
        (
            "lki",
            "lki",
            "selection",
            ("attack", "health", "original_cost", "effective_cost"),
        ),
        ("holder", "live", "holder", ("health",)),
    )
    for property_ in properties
)
_RECEIPTS = tuple(
    _read(
        "read.result." + source + ".v1",
        "card_count" if source != "removed_counter" else "counter_count",
        parameters,
        (
            ImportSignature(
                "receipt",
                "ReceiptId",
                "receipt",
                receipt_result="amount" if source == "removed_counter" else "objects",
            ),
        ),
        ("V.receipt-field",),
    )
    for source, parameters in (
        ("moved_spell", ("kind",)),
        ("moved_trait_card", ("card", "trait")),
        ("moved_matching_cost", ("card",)),
        ("revealed_cost", ("card", "cost")),
        ("returned", ()),
        ("destroyed_amulet", ("kind",)),
        ("discarded", ()),
        ("acted_name", ("name",)),
        ("acted_trait_card", ("card", "trait")),
        ("placed_evolve", ("owner", "zone")),
        ("removed_counter", ("counter",)),
    )
)
_COUNTERS = (
    _read(
        "read.holder.previous_counter.v1",
        "counter_count",
        ("counter",),
        (ImportSignature("subject", "ObjectSet", "lki", "one", "holder"),),
        ("V.counter",),
    ),
)
_VALUE = Parameter("value", "QuantityExpr", "read_value", "")
_OFFSETS = tuple(
    Descriptor(
        "read.offset." + operator + ".v1",
        "quantity.read.context.v1",
        "read_value",
        "Nat",
        "operand",
        (_VALUE, PARAMETERS["offset"]),
        (),
        "A1",
        ("X.math",),
    )
    for operator in ("add", "subtract")
)
_HALF = Descriptor(
    "read.half_up.v1",
    "quantity.read.context.v1",
    "read_value",
    "Nat",
    "operand",
    (_VALUE,),
    (),
    "A1",
    ("X.math", "HalfUp"),
)
_ADDITIONAL = (
    _read(
        "read.distinct_original_cost.zone.v1",
        "cost_kind_count",
        ("owner", "zone", "card"),
        bridge=("V.cost", "X.aggregate", "DistinctCount"),
    ),
    _read(
        "read.subject_player.hand_count.v1",
        "card_count",
        ("zone",),
        (ImportSignature("subject", "ObjectSet", "selection", "one", "selection"),),
        ("V.player", "V.zone-count"),
    ),
    _read(
        "read.event_subject.effective_cost.v1",
        "cost",
        (),
        (ImportSignature("subject", "ObjectSet", "at_event", "one", "event_subject"),),
        ("V.cost", "V.event-field"),
    ),
    *(
        _read(
            "read.result_object." + suffix + ".v1",
            measure,
            parameters,
            (
                ImportSignature(
                    "subject",
                    "ObjectSet",
                    cast("CaptureTime", capture),
                    "one",
                    "selection",
                ),
            ),
            ("V.stats", "S.filter"),
        )
        for suffix, measure, parameters, capture in (
            ("banished_class_attack", "attack", ("class", "kind"), "lki"),
            ("banished_not_class_attack", "attack", ("class", "kind"), "lki"),
            ("entered_trait_health", "health", ("kind", "trait"), "selection"),
        )
    ),
)
DESCRIPTORS = (
    *_FACEUP,
    *_OBJECTS,
    *_RECEIPTS,
    *_COUNTERS,
    *_OFFSETS,
    _HALF,
    *_ADDITIONAL,
)
REGISTRY = DEFAULT_REGISTRY.extend(DESCRIPTORS)
