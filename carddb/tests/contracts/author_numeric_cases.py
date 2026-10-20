"""Independent author-contract cases and explicit decoder correspondence."""

from typing import TYPE_CHECKING

from pydantic import ConfigDict, TypeAdapter

from sve_carddb.contracts import author_expressions as author

if TYPE_CHECKING:
    from pydantic import JsonValue

CONFIG = ConfigDict(regex_engine="python-re")
NUMERIC: dict[str, TypeAdapter[object]] = {
    "A.expr.plus": TypeAdapter(author.ExprPlus),
    "A.expr.minus": TypeAdapter(author.ExprMinus),
    "A.expr.times": TypeAdapter(author.ExprTimes),
    "A.expr.half-up": TypeAdapter(author.ExprHalfUp),
    "A.expr.per-payload": TypeAdapter(author.ExprPerPayload),
    "A.expr.per": TypeAdapter(author.ExprPer),
    "A.expr.min": TypeAdapter(author.ExprMin),
    "A.expr.max": TypeAdapter(author.ExprMax),
    "A.expr.bound": TypeAdapter(author.ExprBound),
    "A.expr.power-of": TypeAdapter(author.ExprPowerOf),
    "A.expr.hp-of": TypeAdapter(author.ExprHpOf),
    "A.expr.cost-of": TypeAdapter(author.ExprCostOf),
    "A.expr.resource-payload": TypeAdapter(author.ExprResourcePayload),
    "A.expr.resource": TypeAdapter(author.ExprResource),
    "A.expr.turn-number-payload": TypeAdapter(author.ExprTurnNumberPayload),
    "A.expr.turn-number": TypeAdapter(author.ExprTurnNumber),
    "A.expr.zone-free-payload": TypeAdapter(author.ExprZoneFreePayload),
    "A.expr.zone-free": TypeAdapter(author.ExprZoneFree),
    "A.expr.numeric-core": TypeAdapter(author.NumericExpr, config=CONFIG),
    "A.cond.cmp-payload": TypeAdapter(author.CondCmpPayload),
    "A.cond.cmp": TypeAdapter(author.CondCmp),
    "A.cond.all": TypeAdapter(author.CondAll),
    "A.cond.any": TypeAdapter(author.CondAny),
    "A.cond.not": TypeAdapter(author.CondNot),
    "A.cond.numeric-core": TypeAdapter(author.NumericCondition, config=CONFIG),
}
NUMERIC_NAMES = {
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
    "A.expr.numeric-core": "NumericExpr",
    "A.cond.cmp-payload": "CondCmpPayload",
    "A.cond.cmp": "CondCmp",
    "A.cond.all": "CondAll",
    "A.cond.any": "CondAny",
    "A.cond.not": "CondNot",
    "A.cond.numeric-core": "NumericCondition",
}
NUMERIC_DEPENDENCIES = {
    "A.param": "ParamRef",
    "A.ref": "Ref",
    "A.player-ref": "PlayerRef",
}
E = "A.expr.numeric-core"
C = "A.cond.numeric-core"
CMP: dict[str, JsonValue] = {"cmp": {"left": -1, "op": "lt", "right": 0}}
EXPRESSIONS: list[tuple[str, dict[str, JsonValue]]] = [
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
]
CONDITIONS: list[tuple[str, dict[str, JsonValue]]] = [
    ("A.cond.cmp", CMP),
    ("A.cond.all", {"all": [CMP]}),
    ("A.cond.all", {"all": [CMP, {"not": CMP}]}),
    ("A.cond.any", {"any": [CMP]}),
    ("A.cond.any", {"any": [CMP, {"not": CMP}]}),
    ("A.cond.not", {"not": CMP}),
]
PAYLOADS: list[tuple[str, dict[str, JsonValue]]] = [
    ("A.expr.per-payload", {"of": "X", "unit": 2}),
    ("A.expr.resource-payload", {"who": "self", "kind": "pp"}),
    ("A.expr.turn-number-payload", {"who": "self"}),
    ("A.expr.zone-free-payload", {"who": "self", "zone": "field"}),
    ("A.cond.cmp-payload", {"left": 0, "op": "eq", "right": 0}),
]
