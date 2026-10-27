"""Recursive author queries; shape validation does not admit execution or scope."""

from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Annotated, Literal, NotRequired, TypedDict

from pydantic import AfterValidator, ConfigDict, Field, with_config

from sve_carddb.contracts.author_leaves import (
    CounterName,
    Int,
    ParamRef,
    Phase,
    Title,
    TrueFlag,
    Zone,
)
from sve_carddb.contracts.author_receipts import SameAs, SnapshotRead
from sve_carddb.contracts.author_references import (
    BindingName,
    EntryPaymentRef,
    LeaderRecipient,
    PlayerRef,
    PlayerSet,
    ReceiptRef,
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
    power_of: Ref | SnapshotRead


class ExprHpOf(RecordData):
    hp_of: Ref | SnapshotRead


class ExprCostOf(RecordData):
    cost_of: Ref | SnapshotRead


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
    | ExprOrigCostOf
    | ExprEventNumber
    | ExprDice
    | ExprXPaid
    | ExprHistoryCount
    | ExprHistorySum
    | CaptureRead
)

type NumericExpr = Annotated[Int, Field(strict=True)] | NumericOperation
type Quantity = Annotated[UInt, Field(strict=True)] | NumericOperation


class CondCmpPayload(RecordData):
    left: NumericExpr
    op: Literal["eq", "ne", "lt", "le", "gt", "ge"]
    right: NumericExpr


class CondCmp(RecordData):
    cmp: CondCmpPayload | PlayerCmpPayload


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
    | CondHistory
    | CondPaid
    | CondPaymentDecided
    | CondChosen
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
    on: Ref | LkiOf
    name: CounterName


class ExprCounter(RecordData):
    counter: ExprCounterPayload


class ExprDistinctNames(RecordData):
    distinct_names: CollectionSource


class ExprDistinctOrigCosts(RecordData):
    distinct_orig_costs: CollectionSource


class ExprAggPayload(RecordData):
    of: CollectionSource
    attr: Literal["cost", "orig_cost", "power", "hp"]
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


SHAPE_CONFIG = ConfigDict(extra="forbid", strict=True, regex_engine="python-re")


@with_config(SHAPE_CONFIG)
class LeaderOrFields(TypedDict):
    who: PlayerRef
    kind: Literal["follower"]
    leader_or: TrueFlag
    zone: NotRequired[Literal["field"]]


type LeaderOr = LeaderOrFields
type RecipientAtom = ObjectAtom | LeaderRecipient | LeaderOr


@with_config(SHAPE_CONFIG)
class RecipientUnionFields(TypedDict):
    any_of: Annotated[
        tuple[RecipientAtom, ...],
        Field(
            min_length=1,
            json_schema_extra={
                "contains": {
                    "anyOf": [
                        {"required": ["leader"]},
                        {"required": ["leaders"]},
                        {"required": ["leader_of"]},
                        {"required": ["leader_or"]},
                    ]
                }
            },
        ),
    ]


def _require_leader(value: RecipientUnionFields) -> RecipientUnionFields:
    # Validated object atoms are dicts; the existing leader atoms are models.
    if not any(
        not isinstance(limb, dict) or limb.get("leader_or") is True
        for limb in value["any_of"]
    ):
        raise ValueError("A recipient union requires a leader limb")
    return value


type RecipientUnion = Annotated[RecipientUnionFields, AfterValidator(_require_leader)]
type RecipientSel = ObjectSel | LeaderRecipient | LeaderOr | RecipientUnion


NONEMPTY_CONFIG = ConfigDict(
    extra="forbid",
    strict=True,
    regex_engine="python-re",
    json_schema_extra={"minProperties": 1},
)


def _nonempty[T: Mapping[str, object]](value: T) -> T:
    if not value:
        raise ValueError("At least one filter is required")
    return value


@with_config(NONEMPTY_CONFIG)
class MaterialFilterFields(TypedDict, total=False):
    name: Text
    zone: Zone
    kind: Code
    trait: Text
    not_class: Text
    orig_cost_eq: Quantity
    orig_cost_ge: Quantity


type MaterialFilter = Annotated[MaterialFilterFields, AfterValidator(_nonempty)]


class CountOfFiltered(RecordData):
    receipt: ReceiptRef
    where: MaterialFilter
    view: Literal["before", "after"]


class ExprCountOf(RecordData):
    count_of: ReceiptRef | CountOfFiltered


@with_config(SHAPE_CONFIG)
class DidFilteredFields(TypedDict):
    result: ReceiptRef
    where: MaterialFilter
    view: NotRequired[Literal["before", "after"]]


type DidFiltered = DidFilteredFields


class CondDid(RecordData):
    did: ReceiptRef | DidFiltered


class LkiOf(RecordData):
    lki_of: Ref


class ExprOrigCostOf(RecordData):
    orig_cost_of: Ref | SnapshotRead


class ExprEventNumber(RecordData):
    event: Literal["amount", "value"]


class ExprDice(RecordData):
    dice: BindingName


class ExprXPaid(RecordData):
    x_paid: BindingName


class PaidReceipt(RecordData):
    receipt: BindingName | EntryPaymentRef


class CondPaymentDecided(RecordData):
    payment_decided: Literal["optional"]


class CondPaid(RecordData):
    paid: Literal["additional", "optional"] | PaidReceipt


class PlayerExpr(RecordData):
    player: PlayerRef


class PlayerCmpPayload(RecordData):
    left: PlayerExpr
    op: Literal["eq", "ne"]
    right: PlayerExpr


EVENT_FILTER_CONFIG = ConfigDict(
    extra="forbid",
    strict=True,
    regex_engine="python-re",
    json_schema_extra={
        **FILTER_CONSTRAINTS,
        "dependentRequired": {"as": ["where"]},
        "minProperties": 1,
    },
)
_EventScope = TypedDict(
    "_EventScope",
    {"where": "Condition", "as": BindingName},
    total=False,
)


@with_config(EVENT_FILTER_CONFIG)
class EventSubjectFields(_CountClassFilter, _EventScope, total=False):
    not_class: Text
    not_trait: Text
    traits_any: Annotated[tuple[Text, ...], Field(min_length=1)]
    not_same_name_as_this: TrueFlag
    cost_eq: UInt
    cost_le: UInt
    cost_ge: UInt
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
    orig_cost_eq: UInt
    orig_cost_le: UInt
    orig_cost_ge: UInt
    hp_le: UInt
    acted: bool
    racing: bool
    token: TrueFlag
    not_token: TrueFlag
    kind: Code
    trait: Text
    traits_all: Annotated[tuple[Text, ...], Field(min_length=1)]
    name: Text


def _validate_event_filter[T: Mapping[str, object]](value: T) -> T:
    return _nonempty(_validate_selection(value))


type EventSubjectFilter = Annotated[
    EventSubjectFields, AfterValidator(_validate_event_filter)
]


@with_config(EVENT_FILTER_CONFIG)
class EventObjectFields(EventSubjectFields, total=False):
    who: PlayerRef
    zone: Zone | Annotated[tuple[Zone, ...], Field(min_length=1)]


type EventObjectSel = Annotated[
    EventObjectFields, AfterValidator(_validate_event_filter)
]
type EventRecipientAtom = EventObjectSel | LeaderRecipient | LeaderOr


@with_config(SHAPE_CONFIG)
class EventRecipientUnionFields(TypedDict):
    any_of: Annotated[
        tuple[EventRecipientAtom, ...],
        Field(
            min_length=1,
            json_schema_extra={
                "contains": {
                    "anyOf": [
                        {"required": ["leader"]},
                        {"required": ["leaders"]},
                        {"required": ["leader_of"]},
                        {"required": ["leader_or"]},
                    ]
                }
            },
        ),
    ]


def _event_leader(value: EventRecipientUnionFields) -> EventRecipientUnionFields:
    if not any(
        not isinstance(limb, dict) or limb.get("leader_or") is True
        for limb in value["any_of"]
    ):
        raise ValueError("A recipient union requires a leader limb")
    return value


type EventRecipientUnion = Annotated[
    EventRecipientUnionFields, AfterValidator(_event_leader)
]
type EventRecipient = Ref | EventRecipientAtom | EventRecipientUnion


@with_config(SHAPE_CONFIG)
class ChosenPayloadFields(TypedDict):
    ref: BindingName
    where: NotRequired[EventSubjectFilter]


type ChosenPayload = ChosenPayloadFields


class CondChosen(RecordData):
    chosen: ChosenPayload


class During(RecordData):
    turn: PlayerRef


class EventBy(RecordData):
    ability_of: EventObjectSel


class MoveOrigin(RecordData):
    who: PlayerRef
    zone: Zone


class NatEq(RecordData):
    eq: UInt


class NatLe(RecordData):
    le: UInt


class NatGe(RecordData):
    ge: UInt


type NatThreshold = NatEq | NatLe | NatGe


class DieEq(RecordData):
    eq: Annotated[int, Field(ge=1, le=6)]


class DieLe(RecordData):
    le: Annotated[int, Field(ge=1, le=6)]


class DieGe(RecordData):
    ge: Annotated[int, Field(ge=1, le=6)]


type DieThreshold = DieEq | DieLe | DieGe


SUBJECT_CONFIG = ConfigDict(
    extra="forbid",
    strict=True,
    regex_engine="python-re",
    json_schema_extra={"not": {"required": ["ref", "sel"]}},
)


def _subject_exclusive[T: Mapping[str, object]](value: T) -> T:
    if "ref" in value and "sel" in value:
        raise ValueError("Event subject reference and filter are exclusive")
    return value


HISTORY_CONFIG = ConfigDict(
    extra="forbid",
    strict=True,
    regex_engine="python-re",
    json_schema_extra={
        "not": {
            "required": ["sel", "where"],
            "properties": {"where": {"required": ["ref"]}},
        }
    },
)


def _history_subject[T: Mapping[str, object]](value: T) -> T:
    where = value.get("where")
    if "sel" in value and isinstance(where, dict) and "ref" in where:
        raise ValueError("History subject reference and filter are exclusive")
    return value


class SinceWindow(RecordData):
    since: BindingName


type HistoryWindow = Literal["this_turn", "game"] | SinceWindow


class HistoryOptions(TypedDict):
    window: HistoryWindow
    distinct: NotRequired[Literal["facts", "subjects"]]
    exclude: NotRequired[Literal["this_execution"]]


class HistorySumOptions(TypedDict):
    window: HistoryWindow
    distinct: NotRequired[Literal["facts"]]
    exclude: NotRequired[Literal["this_execution"]]


_EventEnterFieldFieldsKeys = TypedDict(
    "_EventEnterFieldFieldsKeys", {"from": "NotRequired[Zone]"}
)


@with_config(SUBJECT_CONFIG)
class EventEnterFieldFields(_EventEnterFieldFieldsKeys):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    during: NotRequired[During]
    entered_by: NotRequired[Literal["ability", "play"]]
    by: NotRequired[EventBy]


type EventEnterFieldPayload = Annotated[
    EventEnterFieldFields, AfterValidator(_subject_exclusive)
]


class EventEnterField(RecordData):
    enter_field: EventEnterFieldPayload


@with_config(NONEMPTY_CONFIG)
class EnterFieldHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    during: NotRequired[During]
    entered_by: NotRequired[Literal["ability", "play"]]
    by: NotRequired[EventBy]


type EnterFieldHistoryWhere = Annotated[
    EnterFieldHistoryWhereFields, AfterValidator(_nonempty)
]


_HistoryEnterFieldFieldsKeys = TypedDict(
    "_HistoryEnterFieldFieldsKeys", {"from": "NotRequired[Zone]"}
)


@with_config(HISTORY_CONFIG)
class HistoryEnterFieldFields(HistoryOptions, _HistoryEnterFieldFieldsKeys):
    event: Literal["enter_field"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[EnterFieldHistoryWhere]


type HistoryEnterField = Annotated[
    HistoryEnterFieldFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryEnterFieldConditionFields(HistoryEnterFieldFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryEnterFieldCondition = Annotated[
    HistoryEnterFieldConditionFields, AfterValidator(_history_subject)
]


@with_config(SUBJECT_CONFIG)
class EventLeftFieldFields(TypedDict):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type EventLeftFieldPayload = Annotated[
    EventLeftFieldFields, AfterValidator(_subject_exclusive)
]


class EventLeftField(RecordData):
    left_field: EventLeftFieldPayload


@with_config(NONEMPTY_CONFIG)
class LeftFieldHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type LeftFieldHistoryWhere = Annotated[
    LeftFieldHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryLeftFieldFields(HistoryOptions):
    event: Literal["left_field"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[LeftFieldHistoryWhere]


type HistoryLeftField = Annotated[
    HistoryLeftFieldFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryLeftFieldConditionFields(HistoryLeftFieldFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryLeftFieldCondition = Annotated[
    HistoryLeftFieldConditionFields, AfterValidator(_history_subject)
]


_EventPlacedExFieldsKeys = TypedDict(
    "_EventPlacedExFieldsKeys", {"from": "NotRequired[Zone]"}
)


@with_config(SUBJECT_CONFIG)
class EventPlacedExFields(_EventPlacedExFieldsKeys):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type EventPlacedExPayload = Annotated[
    EventPlacedExFields, AfterValidator(_subject_exclusive)
]


class EventPlacedEx(RecordData):
    placed_ex: EventPlacedExPayload


@with_config(NONEMPTY_CONFIG)
class PlacedExHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type PlacedExHistoryWhere = Annotated[
    PlacedExHistoryWhereFields, AfterValidator(_nonempty)
]


_HistoryPlacedExFieldsKeys = TypedDict(
    "_HistoryPlacedExFieldsKeys", {"from": "NotRequired[Zone]"}
)


@with_config(HISTORY_CONFIG)
class HistoryPlacedExFields(HistoryOptions, _HistoryPlacedExFieldsKeys):
    event: Literal["placed_ex"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[PlacedExHistoryWhere]


type HistoryPlacedEx = Annotated[
    HistoryPlacedExFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryPlacedExConditionFields(HistoryPlacedExFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryPlacedExCondition = Annotated[
    HistoryPlacedExConditionFields, AfterValidator(_history_subject)
]


_EventDiscardedFieldsKeys = TypedDict(
    "_EventDiscardedFieldsKeys", {"from": 'NotRequired[Literal["hand"]]'}
)


@with_config(SUBJECT_CONFIG)
class EventDiscardedFields(_EventDiscardedFieldsKeys):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    by: NotRequired[EventBy]
    during: NotRequired[During]


type EventDiscardedPayload = Annotated[
    EventDiscardedFields, AfterValidator(_subject_exclusive)
]


class EventDiscarded(RecordData):
    discarded: EventDiscardedPayload


@with_config(NONEMPTY_CONFIG)
class DiscardedHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    by: NotRequired[EventBy]
    during: NotRequired[During]


type DiscardedHistoryWhere = Annotated[
    DiscardedHistoryWhereFields, AfterValidator(_nonempty)
]


_HistoryDiscardedFieldsKeys = TypedDict(
    "_HistoryDiscardedFieldsKeys", {"from": 'NotRequired[Literal["hand"]]'}
)


@with_config(HISTORY_CONFIG)
class HistoryDiscardedFields(HistoryOptions, _HistoryDiscardedFieldsKeys):
    event: Literal["discarded"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[DiscardedHistoryWhere]


type HistoryDiscarded = Annotated[
    HistoryDiscardedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryDiscardedConditionFields(HistoryDiscardedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryDiscardedCondition = Annotated[
    HistoryDiscardedConditionFields, AfterValidator(_history_subject)
]


_EventBanishedFieldsKeys = TypedDict(
    "_EventBanishedFieldsKeys", {"from": "NotRequired[Zone]"}
)


@with_config(SUBJECT_CONFIG)
class EventBanishedFields(_EventBanishedFieldsKeys):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type EventBanishedPayload = Annotated[
    EventBanishedFields, AfterValidator(_subject_exclusive)
]


class EventBanished(RecordData):
    banished: EventBanishedPayload


@with_config(NONEMPTY_CONFIG)
class BanishedHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type BanishedHistoryWhere = Annotated[
    BanishedHistoryWhereFields, AfterValidator(_nonempty)
]


_HistoryBanishedFieldsKeys = TypedDict(
    "_HistoryBanishedFieldsKeys", {"from": "NotRequired[Zone]"}
)


@with_config(HISTORY_CONFIG)
class HistoryBanishedFields(HistoryOptions, _HistoryBanishedFieldsKeys):
    event: Literal["banished"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[BanishedHistoryWhere]


type HistoryBanished = Annotated[
    HistoryBanishedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryBanishedConditionFields(HistoryBanishedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryBanishedCondition = Annotated[
    HistoryBanishedConditionFields, AfterValidator(_history_subject)
]


@with_config(SUBJECT_CONFIG)
class EventDestroyedFields(TypedDict):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type EventDestroyedPayload = Annotated[
    EventDestroyedFields, AfterValidator(_subject_exclusive)
]


class EventDestroyed(RecordData):
    destroyed: EventDestroyedPayload


@with_config(NONEMPTY_CONFIG)
class DestroyedHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type DestroyedHistoryWhere = Annotated[
    DestroyedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryDestroyedFields(HistoryOptions):
    event: Literal["destroyed"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[DestroyedHistoryWhere]


type HistoryDestroyed = Annotated[
    HistoryDestroyedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryDestroyedConditionFields(HistoryDestroyedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryDestroyedCondition = Annotated[
    HistoryDestroyedConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventDamagedFields(TypedDict):
    recipient: NotRequired[EventRecipient]
    kind: NotRequired[Literal["attack", "combat", "ability"]]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type EventDamagedPayload = Annotated[
    EventDamagedFields, AfterValidator(_subject_exclusive)
]


class EventDamaged(RecordData):
    damaged: EventDamagedPayload


@with_config(NONEMPTY_CONFIG)
class DamagedHistoryWhereFields(TypedDict):
    recipient: NotRequired[EventRecipient]
    kind: NotRequired[Literal["attack", "combat", "ability"]]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type DamagedHistoryWhere = Annotated[
    DamagedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryDamagedFields(HistoryOptions):
    event: Literal["damaged"]
    where: NotRequired[DamagedHistoryWhere]


type HistoryDamaged = Annotated[HistoryDamagedFields, AfterValidator(_history_subject)]


@with_config(HISTORY_CONFIG)
class HistoryDamagedConditionFields(HistoryDamagedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryDamagedCondition = Annotated[
    HistoryDamagedConditionFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryDamagedSumFields(HistorySumOptions):
    event: Literal["damaged"]
    where: NotRequired[DamagedHistoryWhere]
    field: Literal["amount"]


type HistoryDamagedSum = Annotated[
    HistoryDamagedSumFields, AfterValidator(_history_subject)
]


@with_config(SUBJECT_CONFIG)
class EventDealtDamageFields(TypedDict):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    zone: NotRequired[Zone]
    recipient: NotRequired[EventRecipient]
    kind: NotRequired[Literal["attack", "combat", "ability"]]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type EventDealtDamagePayload = Annotated[
    EventDealtDamageFields, AfterValidator(_subject_exclusive)
]


class EventDealtDamage(RecordData):
    dealt_damage: EventDealtDamagePayload


@with_config(NONEMPTY_CONFIG)
class DealtDamageHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    recipient: NotRequired[EventRecipient]
    kind: NotRequired[Literal["attack", "combat", "ability"]]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type DealtDamageHistoryWhere = Annotated[
    DealtDamageHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryDealtDamageFields(HistoryOptions):
    event: Literal["dealt_damage"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    zone: NotRequired[Zone]
    where: NotRequired[DealtDamageHistoryWhere]


type HistoryDealtDamage = Annotated[
    HistoryDealtDamageFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryDealtDamageConditionFields(HistoryDealtDamageFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryDealtDamageCondition = Annotated[
    HistoryDealtDamageConditionFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryDealtDamageSumFields(HistorySumOptions):
    event: Literal["dealt_damage"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    zone: NotRequired[Zone]
    where: NotRequired[DealtDamageHistoryWhere]
    field: Literal["amount"]


type HistoryDealtDamageSum = Annotated[
    HistoryDealtDamageSumFields, AfterValidator(_history_subject)
]


@with_config(SUBJECT_CONFIG)
class EventAttackedFields(TypedDict):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    during: NotRequired[During]


type EventAttackedPayload = Annotated[
    EventAttackedFields, AfterValidator(_subject_exclusive)
]


class EventAttacked(RecordData):
    attacked: EventAttackedPayload


@with_config(NONEMPTY_CONFIG)
class AttackedHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    during: NotRequired[During]


type AttackedHistoryWhere = Annotated[
    AttackedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryAttackedFields(HistoryOptions):
    event: Literal["attacked"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[AttackedHistoryWhere]


type HistoryAttacked = Annotated[
    HistoryAttackedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryAttackedConditionFields(HistoryAttackedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryAttackedCondition = Annotated[
    HistoryAttackedConditionFields, AfterValidator(_history_subject)
]


@with_config(SUBJECT_CONFIG)
class EventAttackTargetFields(TypedDict):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]


type EventAttackTargetPayload = Annotated[
    EventAttackTargetFields, AfterValidator(_subject_exclusive)
]


class EventAttackTarget(RecordData):
    attack_target: EventAttackTargetPayload


@with_config(NONEMPTY_CONFIG)
class AttackTargetHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]


type AttackTargetHistoryWhere = Annotated[
    AttackTargetHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryAttackTargetFields(HistoryOptions):
    event: Literal["attack_target"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[AttackTargetHistoryWhere]


type HistoryAttackTarget = Annotated[
    HistoryAttackTargetFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryAttackTargetConditionFields(HistoryAttackTargetFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryAttackTargetCondition = Annotated[
    HistoryAttackTargetConditionFields, AfterValidator(_history_subject)
]


@with_config(SUBJECT_CONFIG)
class EventEvolvedFields(TypedDict):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    by: NotRequired[EventBy]


type EventEvolvedPayload = Annotated[
    EventEvolvedFields, AfterValidator(_subject_exclusive)
]


class EventEvolved(RecordData):
    evolved: EventEvolvedPayload


@with_config(NONEMPTY_CONFIG)
class EvolvedHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    by: NotRequired[EventBy]


type EvolvedHistoryWhere = Annotated[
    EvolvedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryEvolvedFields(HistoryOptions):
    event: Literal["evolved"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[EvolvedHistoryWhere]


type HistoryEvolved = Annotated[HistoryEvolvedFields, AfterValidator(_history_subject)]


@with_config(HISTORY_CONFIG)
class HistoryEvolvedConditionFields(HistoryEvolvedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryEvolvedCondition = Annotated[
    HistoryEvolvedConditionFields, AfterValidator(_history_subject)
]


@with_config(SUBJECT_CONFIG)
class EventSuperEvolvedFields(TypedDict):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    by: NotRequired[EventBy]


type EventSuperEvolvedPayload = Annotated[
    EventSuperEvolvedFields, AfterValidator(_subject_exclusive)
]


class EventSuperEvolved(RecordData):
    super_evolved: EventSuperEvolvedPayload


@with_config(NONEMPTY_CONFIG)
class SuperEvolvedHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    by: NotRequired[EventBy]


type SuperEvolvedHistoryWhere = Annotated[
    SuperEvolvedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistorySuperEvolvedFields(HistoryOptions):
    event: Literal["super_evolved"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[SuperEvolvedHistoryWhere]


type HistorySuperEvolved = Annotated[
    HistorySuperEvolvedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistorySuperEvolvedConditionFields(HistorySuperEvolvedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistorySuperEvolvedCondition = Annotated[
    HistorySuperEvolvedConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventPhaseFields(TypedDict):
    which: Phase | ParamRef
    whose: PlayerRef


type EventPhasePayload = Annotated[EventPhaseFields, AfterValidator(_subject_exclusive)]


class EventPhase(RecordData):
    phase: EventPhasePayload


@with_config(NONEMPTY_CONFIG)
class PhaseHistoryWhereFields(TypedDict):
    which: Phase
    whose: PlayerRef


type PhaseHistoryWhere = Annotated[PhaseHistoryWhereFields, AfterValidator(_nonempty)]


@with_config(HISTORY_CONFIG)
class HistoryPhaseFields(HistoryOptions):
    event: Literal["phase"]
    where: PhaseHistoryWhere


type HistoryPhase = Annotated[HistoryPhaseFields, AfterValidator(_history_subject)]


@with_config(HISTORY_CONFIG)
class HistoryPhaseConditionFields(HistoryPhaseFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryPhaseCondition = Annotated[
    HistoryPhaseConditionFields, AfterValidator(_history_subject)
]


_EventPlayedFieldsKeys = TypedDict(
    "_EventPlayedFieldsKeys", {"from": "NotRequired[Zone]"}
)


@with_config(SHAPE_CONFIG)
class EventPlayedFields(_EventPlayedFieldsKeys):
    who: NotRequired[PlayerRef]
    sel: NotRequired[EventSubjectFilter]
    during: NotRequired[During]


type EventPlayedPayload = Annotated[
    EventPlayedFields, AfterValidator(_subject_exclusive)
]


class EventPlayed(RecordData):
    played: EventPlayedPayload


@with_config(NONEMPTY_CONFIG)
class PlayedHistoryWhereFields(TypedDict):
    during: NotRequired[During]


type PlayedHistoryWhere = Annotated[PlayedHistoryWhereFields, AfterValidator(_nonempty)]


_HistoryPlayedFieldsKeys = TypedDict(
    "_HistoryPlayedFieldsKeys", {"from": "NotRequired[Zone]"}
)


@with_config(HISTORY_CONFIG)
class HistoryPlayedFields(HistoryOptions, _HistoryPlayedFieldsKeys):
    event: Literal["played"]
    who: NotRequired[PlayerRef]
    sel: NotRequired[EventSubjectFilter]
    where: NotRequired[PlayedHistoryWhere]


type HistoryPlayed = Annotated[HistoryPlayedFields, AfterValidator(_history_subject)]


@with_config(HISTORY_CONFIG)
class HistoryPlayedConditionFields(HistoryPlayedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryPlayedCondition = Annotated[
    HistoryPlayedConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventAbilityPlayedFields(TypedDict):
    who: NotRequired[PlayerRef]
    holder: NotRequired[Ref | EventObjectSel]


type EventAbilityPlayedPayload = Annotated[
    EventAbilityPlayedFields, AfterValidator(_subject_exclusive)
]


class EventAbilityPlayed(RecordData):
    ability_played: EventAbilityPlayedPayload


@with_config(NONEMPTY_CONFIG)
class AbilityPlayedHistoryWhereFields(TypedDict):
    holder: NotRequired[Ref | EventObjectSel]


type AbilityPlayedHistoryWhere = Annotated[
    AbilityPlayedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryAbilityPlayedFields(HistoryOptions):
    event: Literal["ability_played"]
    who: NotRequired[PlayerRef]
    where: NotRequired[AbilityPlayedHistoryWhere]


type HistoryAbilityPlayed = Annotated[
    HistoryAbilityPlayedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryAbilityPlayedConditionFields(HistoryAbilityPlayedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryAbilityPlayedCondition = Annotated[
    HistoryAbilityPlayedConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventLifeChangedFields(TypedDict):
    who: NotRequired[PlayerRef]
    direction: NotRequired[Literal["up", "down"]]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type EventLifeChangedPayload = Annotated[
    EventLifeChangedFields, AfterValidator(_subject_exclusive)
]


class EventLifeChanged(RecordData):
    life_changed: EventLifeChangedPayload


@with_config(NONEMPTY_CONFIG)
class LifeChangedHistoryWhereFields(TypedDict):
    direction: NotRequired[Literal["up", "down"]]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type LifeChangedHistoryWhere = Annotated[
    LifeChangedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryLifeChangedFields(HistoryOptions):
    event: Literal["life_changed"]
    who: NotRequired[PlayerRef]
    where: NotRequired[LifeChangedHistoryWhere]


type HistoryLifeChanged = Annotated[
    HistoryLifeChangedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryLifeChangedConditionFields(HistoryLifeChangedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryLifeChangedCondition = Annotated[
    HistoryLifeChangedConditionFields, AfterValidator(_history_subject)
]


@with_config(NONEMPTY_CONFIG)
class LifeSumWhereFields(TypedDict):
    direction: Literal["up", "down"]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type LifeSumWhere = Annotated[LifeSumWhereFields, AfterValidator(_nonempty)]


@with_config(HISTORY_CONFIG)
class HistoryLifeChangedSumFields(HistorySumOptions):
    event: Literal["life_changed"]
    who: NotRequired[PlayerRef]
    where: LifeSumWhere
    field: Literal["delta"]


type HistoryLifeChangedSum = Annotated[
    HistoryLifeChangedSumFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventStatChangedFields(TypedDict):
    ref: NotRequired[Ref]
    stat: NotRequired[
        Literal["power", "hp"]
        | Annotated[tuple[Literal["power", "hp"], ...], Field(min_length=1)]
    ]
    direction: NotRequired[Literal["up", "down"]]


type EventStatChangedPayload = Annotated[
    EventStatChangedFields, AfterValidator(_subject_exclusive)
]


class EventStatChanged(RecordData):
    stat_changed: EventStatChangedPayload


@with_config(NONEMPTY_CONFIG)
class StatChangedHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    stat: NotRequired[
        Literal["power", "hp"]
        | Annotated[tuple[Literal["power", "hp"], ...], Field(min_length=1)]
    ]
    direction: NotRequired[Literal["up", "down"]]


type StatChangedHistoryWhere = Annotated[
    StatChangedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryStatChangedFields(HistoryOptions):
    event: Literal["stat_changed"]
    where: NotRequired[StatChangedHistoryWhere]


type HistoryStatChanged = Annotated[
    HistoryStatChangedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryStatChangedConditionFields(HistoryStatChangedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryStatChangedCondition = Annotated[
    HistoryStatChangedConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventCounterChangedFields(TypedDict):
    ref: NotRequired[Ref]
    name: NotRequired[CounterName]
    after: NotRequired[NatThreshold]
    direction: NotRequired[Literal["up", "down"]]


type EventCounterChangedPayload = Annotated[
    EventCounterChangedFields, AfterValidator(_subject_exclusive)
]


class EventCounterChanged(RecordData):
    counter_changed: EventCounterChangedPayload


@with_config(NONEMPTY_CONFIG)
class CounterChangedHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    name: NotRequired[CounterName]
    after: NotRequired[NatThreshold]
    direction: NotRequired[Literal["up", "down"]]


type CounterChangedHistoryWhere = Annotated[
    CounterChangedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryCounterChangedFields(HistoryOptions):
    event: Literal["counter_changed"]
    where: NotRequired[CounterChangedHistoryWhere]


type HistoryCounterChanged = Annotated[
    HistoryCounterChangedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryCounterChangedConditionFields(HistoryCounterChangedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryCounterChangedCondition = Annotated[
    HistoryCounterChangedConditionFields, AfterValidator(_history_subject)
]


@with_config(SUBJECT_CONFIG)
class EventActedFields(TypedDict):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]


type EventActedPayload = Annotated[EventActedFields, AfterValidator(_subject_exclusive)]


class EventActed(RecordData):
    acted: EventActedPayload


@with_config(NONEMPTY_CONFIG)
class ActedHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]


type ActedHistoryWhere = Annotated[ActedHistoryWhereFields, AfterValidator(_nonempty)]


@with_config(HISTORY_CONFIG)
class HistoryActedFields(HistoryOptions):
    event: Literal["acted"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[ActedHistoryWhere]


type HistoryActed = Annotated[HistoryActedFields, AfterValidator(_history_subject)]


@with_config(HISTORY_CONFIG)
class HistoryActedConditionFields(HistoryActedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryActedCondition = Annotated[
    HistoryActedConditionFields, AfterValidator(_history_subject)
]


@with_config(SUBJECT_CONFIG)
class EventStoodFields(TypedDict):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]


type EventStoodPayload = Annotated[EventStoodFields, AfterValidator(_subject_exclusive)]


class EventStood(RecordData):
    stood: EventStoodPayload


@with_config(NONEMPTY_CONFIG)
class StoodHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]


type StoodHistoryWhere = Annotated[StoodHistoryWhereFields, AfterValidator(_nonempty)]


@with_config(HISTORY_CONFIG)
class HistoryStoodFields(HistoryOptions):
    event: Literal["stood"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[StoodHistoryWhere]


type HistoryStood = Annotated[HistoryStoodFields, AfterValidator(_history_subject)]


@with_config(HISTORY_CONFIG)
class HistoryStoodConditionFields(HistoryStoodFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryStoodCondition = Annotated[
    HistoryStoodConditionFields, AfterValidator(_history_subject)
]


@with_config(SUBJECT_CONFIG)
class EventRacedFields(TypedDict):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]


type EventRacedPayload = Annotated[EventRacedFields, AfterValidator(_subject_exclusive)]


class EventRaced(RecordData):
    raced: EventRacedPayload


@with_config(NONEMPTY_CONFIG)
class RacedHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]


type RacedHistoryWhere = Annotated[RacedHistoryWhereFields, AfterValidator(_nonempty)]


@with_config(HISTORY_CONFIG)
class HistoryRacedFields(HistoryOptions):
    event: Literal["raced"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[RacedHistoryWhere]


type HistoryRaced = Annotated[HistoryRacedFields, AfterValidator(_history_subject)]


@with_config(HISTORY_CONFIG)
class HistoryRacedConditionFields(HistoryRacedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryRacedCondition = Annotated[
    HistoryRacedConditionFields, AfterValidator(_history_subject)
]


@with_config(SUBJECT_CONFIG)
class EventDriveGainedFields(TypedDict):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]


type EventDriveGainedPayload = Annotated[
    EventDriveGainedFields, AfterValidator(_subject_exclusive)
]


class EventDriveGained(RecordData):
    drive_gained: EventDriveGainedPayload


@with_config(NONEMPTY_CONFIG)
class DriveGainedHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]


type DriveGainedHistoryWhere = Annotated[
    DriveGainedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryDriveGainedFields(HistoryOptions):
    event: Literal["drive_gained"]
    sel: NotRequired[EventSubjectFilter]
    who: NotRequired[PlayerRef]
    where: NotRequired[DriveGainedHistoryWhere]


type HistoryDriveGained = Annotated[
    HistoryDriveGainedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryDriveGainedConditionFields(HistoryDriveGainedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryDriveGainedCondition = Annotated[
    HistoryDriveGainedConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventUbActivatedFields(TypedDict):
    who: NotRequired[PlayerRef]
    holder: NotRequired[Ref | EventObjectSel]


type EventUbActivatedPayload = Annotated[
    EventUbActivatedFields, AfterValidator(_subject_exclusive)
]


class EventUbActivated(RecordData):
    ub_activated: EventUbActivatedPayload


@with_config(NONEMPTY_CONFIG)
class UbActivatedHistoryWhereFields(TypedDict):
    holder: NotRequired[Ref | EventObjectSel]


type UbActivatedHistoryWhere = Annotated[
    UbActivatedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryUbActivatedFields(HistoryOptions):
    event: Literal["ub_activated"]
    who: NotRequired[PlayerRef]
    where: NotRequired[UbActivatedHistoryWhere]


type HistoryUbActivated = Annotated[
    HistoryUbActivatedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryUbActivatedConditionFields(HistoryUbActivatedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryUbActivatedCondition = Annotated[
    HistoryUbActivatedConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventFusedFields(TypedDict):
    who: NotRequired[PlayerRef]
    material: NotRequired[EventSubjectFilter]


type EventFusedPayload = Annotated[EventFusedFields, AfterValidator(_subject_exclusive)]


class EventFused(RecordData):
    fused: EventFusedPayload


@with_config(NONEMPTY_CONFIG)
class FusedHistoryWhereFields(TypedDict):
    material: NotRequired[EventSubjectFilter]


type FusedHistoryWhere = Annotated[FusedHistoryWhereFields, AfterValidator(_nonempty)]


@with_config(HISTORY_CONFIG)
class HistoryFusedFields(HistoryOptions):
    event: Literal["fused"]
    who: NotRequired[PlayerRef]
    where: NotRequired[FusedHistoryWhere]


type HistoryFused = Annotated[HistoryFusedFields, AfterValidator(_history_subject)]


@with_config(HISTORY_CONFIG)
class HistoryFusedConditionFields(HistoryFusedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryFusedCondition = Annotated[
    HistoryFusedConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventTargetedFields(TypedDict):
    ref: NotRequired[Ref]
    by: NotRequired[EventBy]


type EventTargetedPayload = Annotated[
    EventTargetedFields, AfterValidator(_subject_exclusive)
]


class EventTargeted(RecordData):
    targeted: EventTargetedPayload


@with_config(NONEMPTY_CONFIG)
class TargetedHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    by: NotRequired[EventBy]


type TargetedHistoryWhere = Annotated[
    TargetedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryTargetedFields(HistoryOptions):
    event: Literal["targeted"]
    where: NotRequired[TargetedHistoryWhere]


type HistoryTargeted = Annotated[
    HistoryTargetedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryTargetedConditionFields(HistoryTargetedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryTargetedCondition = Annotated[
    HistoryTargetedConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventDiceRolledFields(TypedDict):
    who: NotRequired[PlayerRef]


type EventDiceRolledPayload = Annotated[
    EventDiceRolledFields, AfterValidator(_subject_exclusive)
]


class EventDiceRolled(RecordData):
    dice_rolled: EventDiceRolledPayload


@with_config(NONEMPTY_CONFIG)
class DiceRolledHistoryWhereFields(TypedDict):
    value: NotRequired[DieThreshold]


type DiceRolledHistoryWhere = Annotated[
    DiceRolledHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryDiceRolledFields(HistoryOptions):
    event: Literal["dice_rolled"]
    who: NotRequired[PlayerRef]
    where: NotRequired[DiceRolledHistoryWhere]


type HistoryDiceRolled = Annotated[
    HistoryDiceRolledFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryDiceRolledConditionFields(HistoryDiceRolledFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryDiceRolledCondition = Annotated[
    HistoryDiceRolledConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventDriveCheckedFields(TypedDict):
    holder: NotRequired[Ref | EventObjectSel]


type EventDriveCheckedPayload = Annotated[
    EventDriveCheckedFields, AfterValidator(_subject_exclusive)
]


class EventDriveChecked(RecordData):
    drive_checked: EventDriveCheckedPayload


@with_config(NONEMPTY_CONFIG)
class DriveCheckedHistoryWhereFields(TypedDict):
    holder: NotRequired[Ref | EventObjectSel]


type DriveCheckedHistoryWhere = Annotated[
    DriveCheckedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryDriveCheckedFields(HistoryOptions):
    event: Literal["drive_checked"]
    where: NotRequired[DriveCheckedHistoryWhere]


type HistoryDriveChecked = Annotated[
    HistoryDriveCheckedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryDriveCheckedConditionFields(HistoryDriveCheckedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryDriveCheckedCondition = Annotated[
    HistoryDriveCheckedConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventDriveTriggerFields(TypedDict):
    who: NotRequired[PlayerRef]


type EventDriveTriggerPayload = Annotated[
    EventDriveTriggerFields, AfterValidator(_subject_exclusive)
]


class EventDriveTrigger(RecordData):
    drive_trigger: EventDriveTriggerPayload


@with_config(HISTORY_CONFIG)
class HistoryDriveTriggerFields(HistoryOptions):
    event: Literal["drive_trigger"]
    who: NotRequired[PlayerRef]


type HistoryDriveTrigger = Annotated[
    HistoryDriveTriggerFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryDriveTriggerConditionFields(HistoryDriveTriggerFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryDriveTriggerCondition = Annotated[
    HistoryDriveTriggerConditionFields, AfterValidator(_history_subject)
]


_EventMovedFieldsKeys = TypedDict(
    "_EventMovedFieldsKeys", {"from": NotRequired[Zone | MoveOrigin]}
)


@with_config(SUBJECT_CONFIG)
class EventMovedFields(_EventMovedFieldsKeys):
    ref: NotRequired[Ref]
    sel: NotRequired[EventSubjectFilter]
    to: NotRequired[Zone]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type EventMovedPayload = Annotated[EventMovedFields, AfterValidator(_subject_exclusive)]


class EventMoved(RecordData):
    moved: EventMovedPayload


@with_config(NONEMPTY_CONFIG)
class MovedHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    during: NotRequired[During]
    by: NotRequired[EventBy]


type MovedHistoryWhere = Annotated[MovedHistoryWhereFields, AfterValidator(_nonempty)]


_HistoryMovedFieldsKeys = TypedDict(
    "_HistoryMovedFieldsKeys", {"from": NotRequired[Zone | MoveOrigin]}
)


@with_config(HISTORY_CONFIG)
class HistoryMovedFields(HistoryOptions, _HistoryMovedFieldsKeys):
    event: Literal["moved"]
    sel: NotRequired[EventSubjectFilter]
    to: NotRequired[Zone]
    where: NotRequired[MovedHistoryWhere]


type HistoryMoved = Annotated[HistoryMovedFields, AfterValidator(_history_subject)]


@with_config(HISTORY_CONFIG)
class HistoryMovedConditionFields(HistoryMovedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryMovedCondition = Annotated[
    HistoryMovedConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventDrewFields(TypedDict):
    who: NotRequired[PlayerRef]
    not_phase: NotRequired[Phase]
    by: NotRequired[EventBy]


type EventDrewPayload = Annotated[EventDrewFields, AfterValidator(_subject_exclusive)]


class EventDrew(RecordData):
    drew: EventDrewPayload


@with_config(NONEMPTY_CONFIG)
class DrewHistoryWhereFields(TypedDict):
    not_phase: NotRequired[Phase]
    by: NotRequired[EventBy]


type DrewHistoryWhere = Annotated[DrewHistoryWhereFields, AfterValidator(_nonempty)]


@with_config(HISTORY_CONFIG)
class HistoryDrewFields(HistoryOptions):
    event: Literal["drew"]
    who: NotRequired[PlayerRef]
    where: NotRequired[DrewHistoryWhere]


type HistoryDrew = Annotated[HistoryDrewFields, AfterValidator(_history_subject)]


@with_config(HISTORY_CONFIG)
class HistoryDrewConditionFields(HistoryDrewFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryDrewCondition = Annotated[
    HistoryDrewConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventEquippedFields(TypedDict):
    equipment: NotRequired[Ref | EventObjectSel]


type EventEquippedPayload = Annotated[
    EventEquippedFields, AfterValidator(_subject_exclusive)
]


class EventEquipped(RecordData):
    equipped: EventEquippedPayload


@with_config(NONEMPTY_CONFIG)
class EquippedHistoryWhereFields(TypedDict):
    equipment: NotRequired[Ref | EventObjectSel]


type EquippedHistoryWhere = Annotated[
    EquippedHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryEquippedFields(HistoryOptions):
    event: Literal["equipped"]
    where: NotRequired[EquippedHistoryWhere]


type HistoryEquipped = Annotated[
    HistoryEquippedFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryEquippedConditionFields(HistoryEquippedFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryEquippedCondition = Annotated[
    HistoryEquippedConditionFields, AfterValidator(_history_subject)
]


@with_config(SHAPE_CONFIG)
class EventFusedAsMaterialFields(TypedDict):
    ref: NotRequired[Ref]
    by: NotRequired[EventBy]


type EventFusedAsMaterialPayload = Annotated[
    EventFusedAsMaterialFields, AfterValidator(_subject_exclusive)
]


class EventFusedAsMaterial(RecordData):
    fused_as_material: EventFusedAsMaterialPayload


@with_config(NONEMPTY_CONFIG)
class FusedAsMaterialHistoryWhereFields(TypedDict):
    ref: NotRequired[Ref]
    by: NotRequired[EventBy]


type FusedAsMaterialHistoryWhere = Annotated[
    FusedAsMaterialHistoryWhereFields, AfterValidator(_nonempty)
]


@with_config(HISTORY_CONFIG)
class HistoryFusedAsMaterialFields(HistoryOptions):
    event: Literal["fused_as_material"]
    where: NotRequired[FusedAsMaterialHistoryWhere]


type HistoryFusedAsMaterial = Annotated[
    HistoryFusedAsMaterialFields, AfterValidator(_history_subject)
]


@with_config(HISTORY_CONFIG)
class HistoryFusedAsMaterialConditionFields(HistoryFusedAsMaterialFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryFusedAsMaterialCondition = Annotated[
    HistoryFusedAsMaterialConditionFields, AfterValidator(_history_subject)
]


class PaymentHistoryWhere(RecordData):
    cost_kind: Literal["earth_rite"]


@with_config(SHAPE_CONFIG)
class HistoryPaymentFields(HistoryOptions):
    event: Literal["payment"]
    who: NotRequired[PlayerRef]
    where: PaymentHistoryWhere


type HistoryPayment = HistoryPaymentFields


@with_config(SHAPE_CONFIG)
class HistoryPaymentConditionFields(HistoryPaymentFields):
    at_least: NotRequired[Quantity]
    at: NotRequired[Literal["event"]]


type HistoryPaymentCondition = HistoryPaymentConditionFields


@with_config(SHAPE_CONFIG)
class HistoryPaymentSumFields(HistorySumOptions):
    event: Literal["payment"]
    who: NotRequired[PlayerRef]
    where: PaymentHistoryWhere
    field: Literal["removed_stack"]


type HistoryPaymentSum = HistoryPaymentSumFields


class CounterReachedPayload(RecordData):
    ref: Ref
    name: CounterName
    value: UInt


class EventCounterReached(RecordData):
    counter_reached: CounterReachedPayload


class EventState(RecordData):
    state: Condition


type EventAtom = (
    EventEnterField
    | EventLeftField
    | EventPlacedEx
    | EventDiscarded
    | EventBanished
    | EventDestroyed
    | EventDamaged
    | EventDealtDamage
    | EventAttacked
    | EventAttackTarget
    | EventEvolved
    | EventSuperEvolved
    | EventPhase
    | EventPlayed
    | EventAbilityPlayed
    | EventLifeChanged
    | EventStatChanged
    | EventCounterChanged
    | EventActed
    | EventStood
    | EventRaced
    | EventDriveGained
    | EventUbActivated
    | EventFused
    | EventTargeted
    | EventDiceRolled
    | EventDriveChecked
    | EventDriveTrigger
    | EventMoved
    | EventDrew
    | EventEquipped
    | EventFusedAsMaterial
    | EventCounterReached
)


class EventAnyOf(RecordData):
    any_of: Annotated[tuple[EventAtom, ...], Field(min_length=1)]


type EventPattern = EventAtom | EventAnyOf | EventState


type HistoryQuery = (
    HistoryEnterField
    | HistoryLeftField
    | HistoryPlacedEx
    | HistoryDiscarded
    | HistoryBanished
    | HistoryDestroyed
    | HistoryDamaged
    | HistoryDealtDamage
    | HistoryAttacked
    | HistoryAttackTarget
    | HistoryEvolved
    | HistorySuperEvolved
    | HistoryPhase
    | HistoryPlayed
    | HistoryAbilityPlayed
    | HistoryLifeChanged
    | HistoryStatChanged
    | HistoryCounterChanged
    | HistoryActed
    | HistoryStood
    | HistoryRaced
    | HistoryDriveGained
    | HistoryUbActivated
    | HistoryFused
    | HistoryTargeted
    | HistoryDiceRolled
    | HistoryDriveChecked
    | HistoryDriveTrigger
    | HistoryMoved
    | HistoryDrew
    | HistoryEquipped
    | HistoryFusedAsMaterial
    | HistoryPayment
)


type HistoryConditionQuery = (
    HistoryEnterFieldCondition
    | HistoryLeftFieldCondition
    | HistoryPlacedExCondition
    | HistoryDiscardedCondition
    | HistoryBanishedCondition
    | HistoryDestroyedCondition
    | HistoryDamagedCondition
    | HistoryDealtDamageCondition
    | HistoryAttackedCondition
    | HistoryAttackTargetCondition
    | HistoryEvolvedCondition
    | HistorySuperEvolvedCondition
    | HistoryPhaseCondition
    | HistoryPlayedCondition
    | HistoryAbilityPlayedCondition
    | HistoryLifeChangedCondition
    | HistoryStatChangedCondition
    | HistoryCounterChangedCondition
    | HistoryActedCondition
    | HistoryStoodCondition
    | HistoryRacedCondition
    | HistoryDriveGainedCondition
    | HistoryUbActivatedCondition
    | HistoryFusedCondition
    | HistoryTargetedCondition
    | HistoryDiceRolledCondition
    | HistoryDriveCheckedCondition
    | HistoryDriveTriggerCondition
    | HistoryMovedCondition
    | HistoryDrewCondition
    | HistoryEquippedCondition
    | HistoryFusedAsMaterialCondition
    | HistoryPaymentCondition
)


type HistorySumQuery = (
    HistoryDamagedSum
    | HistoryDealtDamageSum
    | HistoryLifeChangedSum
    | HistoryPaymentSum
)


class CondHistory(RecordData):
    history: HistoryConditionQuery


class ExprHistoryCount(RecordData):
    history_count: HistoryQuery


class ExprHistorySum(RecordData):
    history_sum: HistorySumQuery


class CapturedPowerOf(RecordData):
    power_of: Ref


class CapturedHpOf(RecordData):
    hp_of: Ref


class CapturedCostOf(RecordData):
    cost_of: Ref


class CapturedOrigCostOf(RecordData):
    orig_cost_of: Ref


type StateNumericRead = (
    ExprCount
    | ExprHand
    | ExprLeaderLife
    | ExprResource
    | ExprTurnNumber
    | ExprZoneFree
    | ExprDistinctNames
    | ExprDistinctOrigCosts
    | ExprAgg
    | CapturedPowerOf
    | CapturedHpOf
    | CapturedCostOf
    | CapturedOrigCostOf
    | ExprHistoryCount
    | ExprHistorySum
)


class TimedCapture(RecordData):
    at: Literal["live", "play", "resolution", "event"]
    value: StateNumericRead


class SelectionCapture(RecordData):
    at: Literal["selection"]
    selection: BindingName
    value: StateNumericRead


type CaptureRead = TimedCapture | SelectionCapture


# Resolve the strongly connected graph under each model's regex configuration.
for _model in tuple(globals().values()):
    if (
        isinstance(_model, type)
        and issubclass(_model, RecordData)
        and _model.__module__ == __name__
    ):
        _model.model_rebuild()
