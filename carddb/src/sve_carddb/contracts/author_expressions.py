"""Numeric author fragments, not complete Expr/Cond or execution admission."""

from typing import Annotated, Literal

from pydantic import ConfigDict, Field

from sve_carddb.contracts.author_leaves import Int, ParamRef
from sve_carddb.contracts.author_references import BindingName, PlayerRef, Ref
from sve_carddb.core.models import RecordData


class ExprPlus(RecordData):
    plus: (
        tuple[NumericExpr, NumericExpr]
        | tuple[ParamRef, NumericExpr]
        | tuple[NumericExpr, ParamRef]
    )


class ExprMinus(RecordData):
    minus: tuple[NumericExpr, NumericExpr | ParamRef]


class ExprTimes(RecordData):
    times: tuple[NumericExpr, NumericExpr]


class ExprHalfUp(RecordData):
    half_up: NumericExpr


class ExprPerPayload(RecordData):
    of: NumericExpr
    unit: Annotated[int, Field(ge=1, le=9007199254740991)]


class ExprPer(RecordData):
    per: ExprPerPayload


class ExprMin(RecordData):
    model_config = ConfigDict(serialize_by_alias=True)

    min_: Annotated[tuple[NumericExpr, ...], Field(min_length=1, alias="min")]


class ExprMax(RecordData):
    model_config = ConfigDict(serialize_by_alias=True)

    max_: Annotated[tuple[NumericExpr, ...], Field(min_length=1, alias="max")]


class ExprBound(RecordData):
    bound: BindingName


class ExprPowerOf(RecordData):
    power_of: Ref


class ExprHpOf(RecordData):
    hp_of: Ref


class ExprCostOf(RecordData):
    cost_of: Ref


class ExprResourcePayload(RecordData):
    who: PlayerRef
    kind: Literal["pp_max", "pp", "ep", "sep"]


class ExprResource(RecordData):
    resource: ExprResourcePayload


class ExprTurnNumberPayload(RecordData):
    who: PlayerRef


class ExprTurnNumber(RecordData):
    turn_number: ExprTurnNumberPayload


class ExprZoneFreePayload(RecordData):
    who: PlayerRef
    zone: Literal["field", "ex"]


class ExprZoneFree(RecordData):
    zone_free: ExprZoneFreePayload


type NumericExpr = (
    Int
    | BindingName
    | ExprPlus
    | ExprMinus
    | ExprTimes
    | ExprHalfUp
    | ExprPer
    | ExprMin
    | ExprMax
    | ExprBound
    | ExprPowerOf
    | ExprHpOf
    | ExprCostOf
    | ExprResource
    | ExprTurnNumber
    | ExprZoneFree
)


class CondCmpPayload(RecordData):
    left: NumericExpr
    op: Literal["eq", "ne", "lt", "le", "gt", "ge"]
    right: NumericExpr


class CondCmp(RecordData):
    cmp: CondCmpPayload


class CondAll(RecordData):
    model_config = ConfigDict(serialize_by_alias=True)

    all_: Annotated[tuple[NumericCondition, ...], Field(min_length=1, alias="all")]


class CondAny(RecordData):
    model_config = ConfigDict(serialize_by_alias=True)

    any_: Annotated[tuple[NumericCondition, ...], Field(min_length=1, alias="any")]


class CondNot(RecordData):
    model_config = ConfigDict(serialize_by_alias=True)

    not_: NumericCondition = Field(alias="not")


type NumericCondition = CondCmp | CondAll | CondAny | CondNot

# Resolve cycles under each model's regex configuration before standalone adapters.
for _model in (
    ExprPlus,
    ExprMinus,
    ExprTimes,
    ExprHalfUp,
    ExprPerPayload,
    ExprPer,
    ExprMin,
    ExprMax,
    CondAll,
    CondAny,
    CondNot,
):
    _model.model_rebuild()
