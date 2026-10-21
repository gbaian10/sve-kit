"""Independent author-contract cases and explicit decoder correspondence."""

from typing import TYPE_CHECKING

from pydantic import ConfigDict

from sve_carddb.contracts import author_queries as author

from .author_adapters import adapters

if TYPE_CHECKING:
    from pydantic import JsonValue

CONFIG = ConfigDict(regex_engine="python-re")
NUMERIC_NAMES = {
    "A.expr.count": "ExprCount",
    "A.expr.plus": "ExprPlus",
    "A.expr.minus": "ExprMinus",
    "A.expr.times": "ExprTimes",
    "A.expr.half-up": "ExprHalfUp",
    "A.expr.per-payload": "ExprPerPayload",
    "A.expr.per": "ExprPer",
    "A.expr.min": "ExprMin",
    "A.expr.max": "ExprMax",
    "A.expr.bound": "ExprBound",
    "A.expr.power-of": "ExprPowerOf",
    "A.expr.hp-of": "ExprHpOf",
    "A.expr.cost-of": "ExprCostOf",
    "A.expr.resource-payload": "ExprResourcePayload",
    "A.expr.resource": "ExprResource",
    "A.expr.turn-number-payload": "ExprTurnNumberPayload",
    "A.expr.turn-number": "ExprTurnNumber",
    "A.expr.zone-free-payload": "ExprZoneFreePayload",
    "A.expr.zone-free": "ExprZoneFree",
    "A.expr.count-of": "ExprCountOf",
    "A.expr.numeric": "NumericExpr",
    "A.expr.numeric-operation": "NumericOperation",
    "A.cond.cmp-payload": "CondCmpPayload",
    "A.cond.cmp": "CondCmp",
    "A.cond.all": "CondAll",
    "A.cond.any": "CondAny",
    "A.cond.not": "CondNot",
    "A.cond.did": "CondDid",
    "A.cond.payment-decided": "CondPaymentDecided",
    "A.cond": "Condition",
}

NUMERIC_DEPENDENCIES = {
    "A.param": "ParamRef",
    "A.ref": "Ref",
    "A.player-ref": "PlayerRef",
    "A.player-set": "PlayerSet",
    "A.same-as": "SameAs",
    "A.snapshot-read": "SnapshotRead",
    "Title": "Title",
}
E = "A.expr.numeric"
C = "A.cond"
CMP: dict[str, JsonValue] = {"cmp": {"left": -1, "op": "lt", "right": 0}}
EXPRESSIONS: list[tuple[str, dict[str, JsonValue]]] = [
    ("A.expr.count", {"count": {"who": "each", "zone": "field"}}),
    ("A.expr.plus", {"plus": [-2, 3]}),
    ("A.expr.plus", {"plus": [{"param": "offset"}, "X"]}),
    ("A.expr.plus", {"plus": ["X", {"param": "offset"}]}),
    ("A.expr.minus", {"minus": ["X", {"param": "offset"}]}),
    ("A.expr.minus", {"minus": [1, 3]}),
    ("A.expr.times", {"times": [{"minus": [1, 3]}, 2]}),
    ("A.expr.half-up", {"half_up": -3}),
    ("A.expr.per", {"per": {"of": -7, "unit": 3}}),
    ("A.expr.min", {"min": [-1]}),
    ("A.expr.min", {"min": [1, -2]}),
    ("A.expr.max", {"max": [0]}),
    ("A.expr.max", {"max": [-1, 2]}),
    ("A.expr.bound", {"bound": "lowest"}),
    ("A.expr.power-of", {"power_of": "this"}),
    ("A.expr.hp-of", {"hp_of": {"follow": "moved_group"}}),
    ("A.expr.cost-of", {"cost_of": "item"}),
    ("A.expr.resource", {"resource": {"who": "self", "kind": "sep"}}),
    ("A.expr.turn-number", {"turn_number": {"who": "self"}}),
    ("A.expr.zone-free", {"zone_free": {"who": "opp", "zone": "ex"}}),
    ("A.expr.count-of", {"count_of": "returned"}),
]
CONDITIONS: list[tuple[str, dict[str, JsonValue]]] = [
    ("A.cond.payment-decided", {"payment_decided": "optional"}),
    ("A.cond.cmp", CMP),
    ("A.cond.all", {"all": [CMP]}),
    ("A.cond.all", {"all": [CMP, {"not": CMP}]}),
    ("A.cond.any", {"any": [CMP]}),
    ("A.cond.any", {"any": [CMP, {"not": CMP}]}),
    ("A.cond.not", {"not": CMP}),
    ("A.cond.did", {"did": "evolved_now"}),
]
PAYLOADS: list[tuple[str, dict[str, JsonValue]]] = [
    ("A.expr.per-payload", {"of": "X", "unit": 2}),
    ("A.expr.resource-payload", {"who": "self", "kind": "pp"}),
    ("A.expr.turn-number-payload", {"who": "self"}),
    ("A.expr.zone-free-payload", {"who": "self", "zone": "field"}),
    ("A.cond.cmp-payload", {"left": 0, "op": "eq", "right": 0}),
]

NUMERIC_NAMES.update(
    {
        "A.expr.hand": "ExprHand",
        "A.expr.leader-life": "ExprLeaderLife",
        "A.expr.counter": "ExprCounter",
        "A.expr.distinct-names": "ExprDistinctNames",
        "A.expr.distinct-orig-costs": "ExprDistinctOrigCosts",
        "A.expr.agg": "ExprAgg",
        "A.expr.counter-payload": "ExprCounterPayload",
        "A.expr.agg-payload": "ExprAggPayload",
        "A.cond.exists": "CondExists",
        "A.cond.in-zone": "CondInZone",
        "A.cond.entered-from": "CondEnteredFrom",
        "A.cond.played-from": "CondPlayedFrom",
        "A.cond.entered-by": "CondEnteredBy",
        "A.cond.is": "CondIs",
        "A.cond.in-zone-payload": "CondInZonePayload",
        "A.cond.from-payload": "CondFromPayload",
        "A.cond.entered-by-payload": "CondEnteredByPayload",
        "A.cond.is-payload": "CondIsPayload",
        "A.cond.on-field": "CondOnField",
        "A.cond.standing": "CondStanding",
        "A.cond.acted": "CondActed",
        "A.cond.evolved": "CondEvolved",
        "A.cond.my-turn": "CondMyTurn",
        "A.cond.opp-turn": "CondOppTurn",
        "A.cond.construction": "CondConstruction",
        "A.cond.combo": "CondCombo",
        "A.cond.spellchain": "CondSpellchain",
        "A.cond.necro": "CondNecro",
        "A.cond.awakened": "CondAwakened",
        "A.cond.crimson": "CondCrimson",
        "A.cond.hand": "CondHand",
        "A.cond.leader-life": "CondLeaderLife",
        "A.cond.zone-free": "CondZoneFree",
        "A.cond.player-eq-threshold": "PlayerEqThreshold",
        "A.cond.player-le-threshold": "PlayerLeThreshold",
        "A.cond.player-ge-threshold": "PlayerGeThreshold",
        "A.cond.zone-free-eq-threshold": "ZoneFreeEqThreshold",
        "A.cond.zone-free-ge-threshold": "ZoneFreeGeThreshold",
        "A.cond.player-threshold": "PlayerThreshold",
        "A.cond.zone-free-threshold": "ZoneFreeThreshold",
    }
)

EXPRESSIONS.extend(
    [
        ("A.expr.hand", {"hand": "self"}),
        ("A.expr.leader-life", {"leader_life": {"master_of": "this"}}),
        ("A.expr.counter", {"counter": {"on": "item", "name": "start"}}),
        ("A.expr.distinct-names", {"distinct_names": {"who": "self", "zone": "ex"}}),
        (
            "A.expr.distinct-orig-costs",
            {"distinct_orig_costs": {"who": "self", "zone": "field"}},
        ),
        (
            "A.expr.agg",
            {
                "agg": {
                    "of": {"who": "self", "zone": "field"},
                    "attr": "power",
                    "fn": "sum",
                }
            },
        ),
    ]
)
CONDITIONS.extend(
    [
        ("A.cond.exists", {"exists": {"in": "shown"}}),
        ("A.cond.in-zone", {"in_zone": {"ref": "this", "zone": "ex"}}),
        ("A.cond.entered-from", {"entered_from": {"ref": "this", "from": "hand"}}),
        ("A.cond.played-from", {"played_from": {"ref": "this", "from": "ex"}}),
        ("A.cond.entered-by", {"entered_by": {"ref": "this", "by": "ability"}}),
        ("A.cond.is", {"is": {"ref": "item", "kind": "follower"}}),
        ("A.cond.on-field", {"on_field": "this"}),
        ("A.cond.standing", {"standing": "item"}),
        ("A.cond.acted", {"acted": "this"}),
        ("A.cond.evolved", {"evolved": "this"}),
        ("A.cond.my-turn", {"my_turn": True}),
        ("A.cond.opp-turn", {"opp_turn": True}),
        ("A.cond.construction", {"construction": "ウマ娘 プリティーダービー"}),
        ("A.cond.combo", {"combo": 3}),
        ("A.cond.spellchain", {"spellchain": 10}),
        ("A.cond.necro", {"necro": 10}),
        ("A.cond.awakened", {"awakened": True}),
        ("A.cond.crimson", {"crimson": True}),
        ("A.cond.hand", {"hand": {"who": "self", "eq": 0}}),
        ("A.cond.leader-life", {"leader_life": {"who": "self", "eq": 0}}),
        ("A.cond.zone-free", {"zone_free": {"who": "self", "zone": "field", "ge": 1}}),
    ]
)
PAYLOADS.extend(
    [
        ("A.expr.counter-payload", {"on": "this", "name": "start"}),
        (
            "A.expr.agg-payload",
            {"of": {"who": "self", "zone": "field"}, "attr": "cost", "fn": "min"},
        ),
        ("A.cond.in-zone-payload", {"ref": "this", "zone": "ex"}),
        ("A.cond.from-payload", {"ref": "this", "from": "hand"}),
        ("A.cond.entered-by-payload", {"ref": "this", "by": "play"}),
        ("A.cond.is-payload", {"ref": "item", "kind": "follower"}),
        ("A.cond.player-eq-threshold", {"who": "self", "eq": 1}),
        ("A.cond.player-le-threshold", {"who": "self", "le": 1}),
        ("A.cond.player-ge-threshold", {"who": "self", "ge": 1}),
        ("A.cond.zone-free-eq-threshold", {"who": "self", "zone": "ex", "eq": 1}),
        ("A.cond.zone-free-ge-threshold", {"who": "self", "zone": "ex", "ge": 1}),
        ("A.cond.player-threshold", {"who": "self", "le": 1}),
        ("A.cond.zone-free-threshold", {"who": "self", "zone": "ex", "eq": 0}),
    ]
)

NUMERIC = adapters(
    {key: (getattr(author, name), name) for key, name in NUMERIC_NAMES.items()}
)
