"""Recursive author queries; shape validation does not admit execution or scope."""

from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Annotated, Literal, NotRequired, TypedDict

from pydantic import AfterValidator, ConfigDict, Field, with_config

from sve_carddb.contracts.author_leaves import (
    CounterName,
    Int,
    ParamRef,
    Title,
    TrueFlag,
    Zone,
)
from sve_carddb.contracts.author_receipts import (
    CondDid,
    ExprCountOf,
    SameAs,
    SnapshotRead,
)
from sve_carddb.contracts.author_references import (
    BindingName,
    PlayerRef,
    PlayerSet,
    Ref,
)
from sve_carddb.contracts.four_layer import Code
from sve_carddb.core.models import RecordData, Text, UInt

if TYPE_CHECKING:
    from pydantic.config import JsonDict


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


type NumericOperation = (
    BindingName
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
    | ExprCountOf
    | ExprCount
    | ExprHand
    | ExprLeaderLife
    | ExprCounter
    | ExprDistinctNames
    | ExprDistinctOrigCosts
    | ExprAgg
)

type NumericExpr = Annotated[Int, Field(strict=True)] | NumericOperation
type Quantity = Annotated[UInt, Field(strict=True)] | NumericOperation


class CondCmpPayload(RecordData):
    left: NumericExpr
    op: Literal["eq", "ne", "lt", "le", "gt", "ge"]
    right: NumericExpr


class CondCmp(RecordData):
    cmp: CondCmpPayload


class CondAll(RecordData):
    model_config = ConfigDict(serialize_by_alias=True)

    all_: Annotated[tuple[Condition, ...], Field(min_length=1, alias="all")]


class CondAny(RecordData):
    model_config = ConfigDict(serialize_by_alias=True)

    any_: Annotated[tuple[Condition, ...], Field(min_length=1, alias="any")]


class CondNot(RecordData):
    model_config = ConfigDict(serialize_by_alias=True)

    not_: Condition = Field(alias="not")


type Condition = (
    CondCmp
    | CondAll
    | CondAny
    | CondNot
    | CondDid
    | CondOnField
    | CondStanding
    | CondActed
    | CondEvolved
    | CondMyTurn
    | CondOppTurn
    | CondConstruction
    | CondCombo
    | CondSpellchain
    | CondNecro
    | CondAwakened
    | CondCrimson
    | CondExists
    | CondInZone
    | CondEnteredFrom
    | CondPlayedFrom
    | CondEnteredBy
    | CondIs
    | CondHand
    | CondLeaderLife
    | CondZoneFree
)


FILTER_CONSTRAINTS: JsonDict = {
    "not": {
        "anyOf": [
            {"required": ["token", "not_token"]},
            {"required": ["name", "names"]},
            {"required": ["name", "name_contains"]},
            {"required": ["names", "name_contains"]},
        ]
    }
}


FILTER_CONFIG = ConfigDict(
    extra="forbid",
    strict=True,
    regex_engine="python-re",
    serialize_by_alias=True,
    json_schema_extra=FILTER_CONSTRAINTS,
)


_ClassFilter = TypedDict("_ClassFilter", {"class": Text | ParamRef}, total=False)
_CountClassFilter = TypedDict("_CountClassFilter", {"class": Text}, total=False)


class _SharedFilterFields(TypedDict, total=False):
    not_class: Text
    not_trait: Text
    traits_any: Annotated[tuple[Text, ...], Field(min_length=1)]
    not_same_name_as_this: TrueFlag
    cost_eq: Quantity
    cost_le: Quantity
    cost_ge: Quantity
    keyword: Text
    has_trigger: TrueFlag
    has_evolve: TrueFlag
    face_up: bool
    boxed: bool
    evolved: bool
    entered_this_turn: TrueFlag
    advance_follower: TrueFlag
    names: Annotated[tuple[Text, ...], Field(min_length=1)]
    name_contains: Text
    other: TrueFlag
    orig_cost_eq: Quantity
    orig_cost_le: Quantity
    orig_cost_ge: Quantity
    hp_le: Quantity
    acted: bool
    racing: bool
    token: TrueFlag
    not_token: TrueFlag


@with_config(FILTER_CONFIG)
class ObjectFilterFields(_SharedFilterFields, _ClassFilter, total=False):
    power_max: TrueFlag
    kind: Code | ParamRef | SameAs
    trait: Text | ParamRef
    traits_all: Annotated[tuple[Text | ParamRef, ...], Field(min_length=1)]
    name: Text | ParamRef | SameAs


@with_config(FILTER_CONFIG)
class CountFilterFields(_SharedFilterFields, _CountClassFilter, total=False):
    # Shared attribute names do not grant Sel's parameter-source permissions.
    kind: Code | SameAs
    trait: Text
    traits_all: Annotated[tuple[Text, ...], Field(min_length=1)]
    name: Text | SameAs


def validate_object_filters[T: Mapping[str, object]](value: T) -> T:
    """Reject contradictory or overlapping author filter spellings."""
    if "token" in value and "not_token" in value:
        raise ValueError("Token filters are exclusive")
    if sum(key in value for key in ("name", "names", "name_contains")) > 1:
        raise ValueError("Name filters are exclusive")
    return value


SELECTION_CONFIG = ConfigDict(
    extra="forbid",
    strict=True,
    regex_engine="python-re",
    serialize_by_alias=True,
    json_schema_extra={
        **FILTER_CONSTRAINTS,
        "dependentRequired": {"as": ["where"]},
    },
)


_SelectionScope = TypedDict(
    "_SelectionScope",
    {"except": "ObjectSel", "where": "Condition", "as": BindingName},
    total=False,
)
_InSource = TypedDict("_InSource", {"in": Ref | SnapshotRead})


@with_config(SELECTION_CONFIG)
class ObjectSelectionFields(ObjectFilterFields, _SelectionScope):
    pass


@with_config(SELECTION_CONFIG)
class ZoneObjectsFields(ObjectSelectionFields):
    who: PlayerRef | PlayerSet | ParamRef
    zone: NotRequired[
        Zone | Annotated[tuple[Zone, ...], Field(min_length=1)] | ParamRef
    ]


@with_config(SELECTION_CONFIG)
class InObjectsFields(ObjectSelectionFields, _InSource):
    pass


def _validate_selection[T: Mapping[str, object]](value: T) -> T:
    validate_object_filters(value)
    if "as" in value and "where" not in value:
        raise ValueError("A local alias requires where")
    return value


type ZoneObjects = Annotated[ZoneObjectsFields, AfterValidator(_validate_selection)]
type InObjects = Annotated[InObjectsFields, AfterValidator(_validate_selection)]
type ObjectAtom = ZoneObjects | InObjects


@with_config(ConfigDict(extra="forbid", strict=True, regex_engine="python-re"))
class ObjectUnionFields(TypedDict):
    any_of: Annotated[tuple[ObjectAtom, ...], Field(min_length=1)]


type ObjectUnion = ObjectUnionFields
type ObjectSel = ObjectAtom | ObjectUnion

_CollectionScope = TypedDict(
    "_CollectionScope",
    {"except": "CollectionSource", "where": "Condition", "as": BindingName},
    total=False,
)


@with_config(SELECTION_CONFIG)
class CollectionFilterFields(CountFilterFields, _CollectionScope):
    pass


@with_config(SELECTION_CONFIG)
class CountObjectsFields(CollectionFilterFields):
    who: PlayerRef | PlayerSet | ParamRef
    zone: Zone | Annotated[tuple[Zone, ...], Field(min_length=1)] | ParamRef


@with_config(SELECTION_CONFIG)
class CollectionObjectsFields(CollectionFilterFields):
    who: PlayerRef | PlayerSet
    zone: Zone | Annotated[tuple[Zone, ...], Field(min_length=1)]


@with_config(SELECTION_CONFIG)
class CollectionInFields(CollectionFilterFields, _InSource):
    pass


type CountObjects = Annotated[CountObjectsFields, AfterValidator(_validate_selection)]
type CollectionObjects = Annotated[
    CollectionObjectsFields, AfterValidator(_validate_selection)
]
type CollectionIn = Annotated[CollectionInFields, AfterValidator(_validate_selection)]
type CountAtom = CountObjects | CollectionIn
type CollectionAtom = CollectionObjects | CollectionIn


class CountUnion(RecordData):
    any_of: Annotated[tuple[CountAtom, ...], Field(min_length=1)]


class CollectionUnion(RecordData):
    any_of: Annotated[tuple[CollectionAtom, ...], Field(min_length=1)]


type CountSource = CountAtom | CountUnion
type CollectionSource = CollectionAtom | CollectionUnion


class ExprCount(RecordData):
    count: CountSource


class ExprHand(RecordData):
    hand: PlayerRef


class ExprLeaderLife(RecordData):
    leader_life: PlayerRef


class ExprCounterPayload(RecordData):
    on: Ref
    name: CounterName


class ExprCounter(RecordData):
    counter: ExprCounterPayload


class ExprDistinctNames(RecordData):
    distinct_names: CollectionSource


class ExprDistinctOrigCosts(RecordData):
    distinct_orig_costs: CollectionSource


class ExprAggPayload(RecordData):
    of: CollectionSource
    attr: Literal["cost", "orig_cost", "power"]
    fn: Literal["sum", "min", "max"]


class ExprAgg(RecordData):
    agg: ExprAggPayload


class CondExists(RecordData):
    exists: ObjectSel


class CondInZonePayload(RecordData):
    ref: Ref
    zone: Zone


class CondInZone(RecordData):
    in_zone: CondInZonePayload


class CondFromPayload(RecordData):
    model_config = ConfigDict(serialize_by_alias=True)

    ref: Ref
    from_: Zone = Field(alias="from")


class CondEnteredFrom(RecordData):
    entered_from: CondFromPayload


class CondPlayedFrom(RecordData):
    played_from: CondFromPayload


class CondEnteredByPayload(RecordData):
    ref: Ref
    by: Literal["ability", "play"]


class CondEnteredBy(RecordData):
    entered_by: CondEnteredByPayload


class CondIsPayload(RecordData):
    ref: Ref
    kind: Code


class CondIs(RecordData):
    model_config = ConfigDict(serialize_by_alias=True)

    is_: CondIsPayload = Field(alias="is")


class CondOnField(RecordData):
    on_field: Ref


class CondStanding(RecordData):
    standing: Ref


class CondActed(RecordData):
    acted: Ref


class CondEvolved(RecordData):
    evolved: Ref


class CondMyTurn(RecordData):
    my_turn: TrueFlag


class CondOppTurn(RecordData):
    opp_turn: TrueFlag


class CondConstruction(RecordData):
    construction: Title


class CondCombo(RecordData):
    combo: Annotated[int, Field(ge=1, le=9007199254740991)]


class CondSpellchain(RecordData):
    spellchain: Annotated[int, Field(ge=1, le=9007199254740991)]


class CondNecro(RecordData):
    necro: Annotated[int, Field(ge=1, le=9007199254740991)]


class CondAwakened(RecordData):
    awakened: TrueFlag


class CondCrimson(RecordData):
    crimson: TrueFlag


class PlayerEqThreshold(RecordData):
    who: PlayerRef
    eq: Quantity


class PlayerLeThreshold(RecordData):
    who: PlayerRef
    le: Quantity


class PlayerGeThreshold(RecordData):
    who: PlayerRef
    ge: Quantity


class ZoneFreeEqThreshold(RecordData):
    who: PlayerRef
    zone: Literal["field", "ex"]
    eq: Quantity


class ZoneFreeGeThreshold(RecordData):
    who: PlayerRef
    zone: Literal["field", "ex"]
    ge: Quantity


type PlayerThreshold = PlayerEqThreshold | PlayerLeThreshold | PlayerGeThreshold
type ZoneFreeThreshold = ZoneFreeEqThreshold | ZoneFreeGeThreshold


class CondHand(RecordData):
    hand: PlayerThreshold


class CondLeaderLife(RecordData):
    leader_life: PlayerThreshold


class CondZoneFree(RecordData):
    zone_free: ZoneFreeThreshold


# Resolve the strongly connected graph under each model's regex configuration.
for _model in tuple(globals().values()):
    if (
        isinstance(_model, type)
        and issubclass(_model, RecordData)
        and _model.__module__ == __name__
    ):
        _model.model_rebuild()
