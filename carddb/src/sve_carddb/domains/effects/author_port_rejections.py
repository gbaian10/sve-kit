"""Classify unsupported source ports without proving a producer or execution scope."""

from typing import TYPE_CHECKING, Literal

from sve_carddb.core.json import canonical, digest

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Port, PortValue

type Direction = Literal["imports", "exports"]
type Signature = tuple[str, ...]

# These are observed source interfaces, not a second operation recognizer. Exact
# provider, generation, scope and relation evidence remains the source layer's job.
_IMPORTS: dict[tuple[str, str], tuple[str, tuple[Signature, ...]]] = {
    ("ReceiptId", "read_receipt"): (
        "receipt",
        (("receipt", "objects"), ("receipt", "amount")),
    ),
    ("ReceiptId", "payment_complete"): ("payment", (("receipt", "payment_complete"),)),
    ("ObjectSet", "read_subject"): (
        "object",
        (
            ("live", "holder"),
            ("selection", "selection"),
            ("lki", "selection"),
            ("at_event", "event_subject"),
        ),
    ),
    ("ObjectSet", "read_holder"): ("object", (("live", "holder"), ("lki", "holder"))),
    ("ObjectSet", "name_source"): (
        "object",
        (("live", "holder"), ("selection", "selection"), ("lki", "event_subject")),
    ),
    ("ObjectSet", "entry_subject"): (
        "object",
        (("at_event", "holder"), ("at_event", "event_subject")),
    ),
    ("ObjectSet", "play_subject"): (
        "object",
        (("at_event", "holder"), ("at_event", "event_subject")),
    ),
    ("ObjectSet", "history_subject"): ("object", (("current_turn", "holder"),)),
    ("ObjectSet", "highest_satisfied_holder"): ("object", (("live", "selection"),)),
    ("ObjectSet", "entry_ability_source"): (
        "object",
        (("at_event", "direct_ability_source"),),
    ),
    ("PlayerRef", "read_player"): ("player", (("live",), ("receipt",))),
    ("PlayerRef", "source_actor"): ("player", (("live",),)),
    ("PlayerRef", "history_actor"): ("player", (("current_turn",),)),
    ("PlayerRef", "history_resource_owner"): ("player", (("current_turn",),)),
    ("PlayerRef", "history_subject_owner"): ("player", (("current_turn",),)),
    ("CapturedValue", "declared_x"): (
        "capture",
        (("play_parameter", "Nat", "declared_x"),),
    ),
    ("CapturedValue", "declared_dice_face"): (
        "capture",
        (("declaration", "Nat", "declared_dice_face"),),
    ),
    ("CapturedValue", "dice_result"): ("capture", (("receipt", "Nat", "die_face"),)),
    ("CapturedValue", "optional_cost_payment"): (
        "capture",
        (("play_parameter", "Bool", "payment_complete"),),
    ),
    ("ChoiceContext", "choice_context"): ("choice", (("play_parameter",),)),
    ("EventRef", "entry_fact"): ("event", (("at_event", "EnterField"),)),
    ("EventRef", "play_fact"): ("event", (("at_event", "Play"),)),
    ("EventRef", "read_event"): ("event", (("at_event", "Damage"),)),
    ("EventRef", "dice_event"): ("event", (("at_event", "DiceRoll"),)),
    ("AbilitySetRef", "ability_source"): (
        "ability",
        (("selection", "term:event.evolve"),),
    ),
}
_RULES = {("imports", *key): value for key, value in _IMPORTS.items()}
_RULES["exports", "CapturedValue", "optional_cost_payment"] = _IMPORTS[
    "CapturedValue", "optional_cost_payment"
]
for _role in ("pool_allocate_any", "pool_allocate_exact", "pool_fill_battlefield"):
    _RULES["imports", "PoolContext", _role] = (
        "pool",
        (("selection", "generated"), ("selection", "search_result")),
    )
    _RULES["exports", "ReceiptId", _role] = ("pool", (("receipt", "objects"),))
_RULES["imports", "PoolContext", "pool_fill_battlefield"] = (
    "pool",
    (("selection", "generated"),),
)
for _role in ("pool_move_remainder", "pool_allocate_any", "pool_allocate_exact"):
    _RULES["imports", "ObjectSet", _role] = ("pool", (("receipt", "pool_remainder"),))
_RULES["exports", "PoolContext", "pool_source"] = (
    "pool",
    (("selection", "generated"), ("selection", "search_result")),
)
_RULES["exports", "ObjectSet", "pool_remaining_after_first"] = (
    "pool",
    (("receipt", "pool_remainder"),),
)
_RULES["exports", "ReceiptId", "pool_search_result"] = (
    "pool",
    (("receipt", "objects"),),
)

PORT_REJECTION_RECIPE = digest(
    canonical(
        {
            "version": "author-port-rejections-v1",
            "priority": ["evidence", "role", "signature", "producer_mapping"],
            "value_kind": "must_equal_port_type",
            "rules": [
                {
                    "key": list(key),
                    "reason": reason,
                    "signatures": [list(s) for s in signatures],
                }
                for key, (reason, signatures) in sorted(_RULES.items())
            ],
        }
    )
)


def _details(value: PortValue) -> tuple[str, ...]:
    details: tuple[str, ...] = ()
    match value.kind:
        case "ReceiptId":
            details = (value.result,)
        case "ObjectSet":
            details = (value.origin,)
        case "CapturedValue":
            details = (value.result_type, value.measure)
        case "EventRef":
            details = (value.event_kind,)
        case "PoolContext":
            details = (value.source_kind,)
        case "AbilitySetRef":
            details = (value.ability_kind,)
    return details


def unsupported_port_reason(port: Port, *, direction: Direction) -> str:
    """Always reject a typed source port; callers retain earlier pending/errors."""
    evidence = port.evidence
    if evidence is None:
        return "unsupported_port_evidence"
    rule = _RULES.get((direction, port.type, port.source_role))
    if rule is None:
        return "unsupported_port_role"
    reason, signatures = rule
    if (
        evidence.value.kind != port.type
        or (evidence.capture, *_details(evidence.value)) not in signatures
    ):
        return "unsupported_port_signature"
    return f"unsupported_{reason}_producer_mapping"
