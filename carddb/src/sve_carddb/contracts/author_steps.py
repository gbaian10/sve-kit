"""Closed action and recursive step fragments, without scope or execution admission."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Annotated, Literal, NotRequired, TypedDict

from pydantic import AfterValidator, ConfigDict, Field, with_config

from sve_carddb.contracts.author_costs import Cost  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_effects import (
    EffectObject,
    EffectPlayers,
    EffectRecipient,
    EffectSpec,
    TriggerKind,
)
from sve_carddb.contracts.author_leaves import CounterName, ParamRef, TrueFlag, Zone
from sve_carddb.contracts.author_quantities import DeckSlice, SelectionQuantity  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_queries import (
    Condition,
    NumericExpr,
    ObjectSel,
    Quantity,
)
from sve_carddb.contracts.author_references import BindingName, PlayerRef, Ref, Until
from sve_carddb.contracts.author_selection_parents import DistinctBy, TargetGroupFilter  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.four_layer import Code  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.core.models import Text

CONFIG = ConfigDict(extra="forbid", strict=True, regex_engine="python-re")


def _single[T: Mapping[str, object]](value: T) -> T:
    if len(value) != 1:
        raise ValueError("Exactly one operation is required")
    return value


def _stats[T: Mapping[str, object]](value: T) -> T:
    if not ("power" in value or "hp" in value):
        raise ValueError("At least one stat is required")
    return value


def _damage[T: Mapping[str, object]](value: T) -> T:
    if "distribute_at" in value and "distribute" not in value:
        raise ValueError("Distribution timing requires distributed damage")
    return value


def _move[T: Mapping[str, object]](value: T) -> T:
    destination = value["to"]
    if isinstance(destination, str):
        if ("position" in value or "order" in value) and destination != "deck":
            raise ValueError("Position and ordering require a deck destination")
        if "acted" in value and destination != "field":
            raise ValueError("Entry posture requires the field")
    if "order" in value and "position" not in value:
        raise ValueError("Ordering requires a position")
    return value


def _replacement[T: Mapping[str, object]](value: T) -> T:
    changed = value.keys() - {"if", "after"}
    if not changed or ("replace_with" in changed and len(changed) != 1):
        raise ValueError("Use differences or one complete replacement")
    if "n" in changed and "upto" in changed:
        raise ValueError("Quantity alternatives are exclusive")
    return value


def _create[T: Mapping[str, object]](value: T) -> T:
    quantities = value.keys() & {"n", "upto", "any"}
    if len(quantities) > 1:
        raise ValueError("Creation quantity alternatives are exclusive")
    token = value["token"]
    if isinstance(token, Mapping) and "allocate" in token and quantities:
        raise ValueError("Allocation owns its total quantity")
    if "acted" in value and isinstance(value["to"], str) and value["to"] != "field":
        raise ValueError("Entry posture requires the field")
    return value


type CountAmount = Quantity | ParamRef


type DamageAmount = Quantity | ParamRef


type RecoveryDelta = NumericExpr | ParamRef


@with_config(CONFIG)
class NthPositionFields(TypedDict):
    nth: Annotated[int, Field(strict=True, ge=1, le=9007199254740991)]


type NthPosition = NthPositionFields


type Placement = Literal["top", "bottom"] | NthPosition


@with_config(CONFIG)
class PositionChoiceFields(TypedDict):
    one_of: Annotated[tuple[Placement, ...], Field(min_length=2)]

    by: PlayerRef


type PositionChoice = PositionChoiceFields


@with_config(CONFIG)
class PositionChosenFields(TypedDict):
    chosen: BindingName


type PositionChosen = PositionChosenFields


type Position = Placement | PositionChoice | PositionChosen


type MoveOrder = Literal["chooser", "shuffle"]


_ChooseExactKeys = TypedDict("_ChooseExactKeys", {"from": ObjectSel})


@with_config(CONFIG)
class ChooseExactFields(_ChooseExactKeys):
    bind: BindingName

    n: SelectionQuantity

    by: NotRequired[PlayerRef]

    may: NotRequired[bool]

    reveal: NotRequired[bool]

    order: NotRequired[bool]

    position: NotRequired[Literal["top_or_bottom"]]

    cost_sum_le: NotRequired[Quantity]

    distinct_by: NotRequired[DistinctBy]

    groups: NotRequired[Annotated[tuple[TargetGroupFilter, ...], Field(min_length=2)]]


type ChooseExact = ChooseExactFields


_ChooseUpToKeys = TypedDict("_ChooseUpToKeys", {"from": ObjectSel})


@with_config(CONFIG)
class ChooseUpToFields(_ChooseUpToKeys):
    bind: BindingName

    upto: SelectionQuantity

    by: NotRequired[PlayerRef]

    may: NotRequired[bool]

    reveal: NotRequired[bool]

    order: NotRequired[bool]

    position: NotRequired[Literal["top_or_bottom"]]

    cost_sum_le: NotRequired[Quantity]

    distinct_by: NotRequired[DistinctBy]

    groups: NotRequired[Annotated[tuple[TargetGroupFilter, ...], Field(min_length=2)]]


type ChooseUpTo = ChooseUpToFields


_ChooseAnyKeys = TypedDict("_ChooseAnyKeys", {"from": ObjectSel})


@with_config(CONFIG)
class ChooseAnyFields(_ChooseAnyKeys):
    bind: BindingName

    any: TrueFlag

    by: NotRequired[PlayerRef]

    may: NotRequired[bool]

    reveal: NotRequired[bool]

    order: NotRequired[bool]

    position: NotRequired[Literal["top_or_bottom"]]

    cost_sum_le: NotRequired[Quantity]

    distinct_by: NotRequired[DistinctBy]

    groups: NotRequired[Annotated[tuple[TargetGroupFilter, ...], Field(min_length=2)]]


type ChooseAny = ChooseAnyFields


type Choose = ChooseExact | ChooseUpTo | ChooseAny


@with_config(CONFIG)
class SearchPartitionPartFields(TypedDict):
    zone: Zone

    n: Quantity


type SearchPartitionPart = SearchPartitionPartFields


@with_config(CONFIG)
class SearchPartitionFields(TypedDict):
    order: Literal["printed"]

    partition: Annotated[tuple[SearchPartitionPart, ...], Field(min_length=2)]


type SearchPartition = SearchPartitionFields


_SearchDestinationReplacementKeys = TypedDict(
    "_SearchDestinationReplacementKeys", {"if": Condition}
)


@with_config(CONFIG)
class SearchDestinationReplacementFields(_SearchDestinationReplacementKeys):
    zone: Zone

    may: NotRequired[bool]


type SearchDestinationReplacement = SearchDestinationReplacementFields


@with_config(CONFIG)
class SearchConditionalDestinationFields(TypedDict):
    zone: Zone

    instead: SearchDestinationReplacement


type SearchConditionalDestination = SearchConditionalDestinationFields


type SearchDestination = (
    Zone | ParamRef | SearchPartition | SearchConditionalDestination
)


@with_config(CONFIG)
class SearchPerGroupFields(TypedDict):
    per_group: Annotated[tuple[Zone, ...], Field(min_length=2)]


type SearchPerGroup = SearchPerGroupFields


type GroupedSearchDestination = SearchDestination | SearchPerGroup


@with_config(CONFIG)
class SearchGroupExactFields(TypedDict):
    sel: ObjectSel

    n: Quantity


type SearchGroupExact = SearchGroupExactFields


@with_config(CONFIG)
class SearchGroupUpToFields(TypedDict):
    sel: ObjectSel

    upto: Quantity


type SearchGroupUpTo = SearchGroupUpToFields


type SearchGroup = SearchGroupExact | SearchGroupUpTo


_DamageInsteadKeys = TypedDict("_DamageInsteadKeys", {"if": Condition})


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "anyOf": [
                {"required": ["to"]},
                {"required": ["n"]},
                {"required": ["replace_with"]},
            ],
            "not": {
                "anyOf": [
                    {"required": ["replace_with", "to"]},
                    {"required": ["replace_with", "n"]},
                ]
            },
        },
    )
)
class DamageInsteadFields(_DamageInsteadKeys):
    to: NotRequired[EffectRecipient]

    n: NotRequired[DamageAmount]

    replace_with: NotRequired[Action]

    after: NotRequired[Steps]


type DamageInstead = Annotated[DamageInsteadFields, AfterValidator(_replacement)]


_MoveInsteadKeys = TypedDict("_MoveInsteadKeys", {"if": Condition})


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "anyOf": [
                {"required": ["ref"]},
                {"required": ["to"]},
                {"required": ["position"]},
                {"required": ["order"]},
                {"required": ["acted"]},
                {"required": ["replace_with"]},
            ],
            "not": {
                "anyOf": [
                    {"required": ["replace_with", "ref"]},
                    {"required": ["replace_with", "to"]},
                    {"required": ["replace_with", "position"]},
                    {"required": ["replace_with", "order"]},
                    {"required": ["replace_with", "acted"]},
                ]
            },
        },
    )
)
class MoveInsteadFields(_MoveInsteadKeys):
    ref: NotRequired[EffectObject]

    to: NotRequired[Zone | ParamRef]

    position: NotRequired[Position]

    order: NotRequired[MoveOrder]

    acted: NotRequired[bool]

    replace_with: NotRequired[Action]

    after: NotRequired[Steps]


type MoveInstead = Annotated[MoveInsteadFields, AfterValidator(_replacement)]


_BuffInsteadKeys = TypedDict("_BuffInsteadKeys", {"if": Condition})


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "anyOf": [
                {"required": ["power"]},
                {"required": ["hp"]},
                {"required": ["replace_with"]},
            ],
            "not": {
                "anyOf": [
                    {"required": ["replace_with", "power"]},
                    {"required": ["replace_with", "hp"]},
                ]
            },
        },
    )
)
class BuffInsteadFields(_BuffInsteadKeys):
    power: NotRequired[NumericExpr]

    hp: NotRequired[NumericExpr]

    replace_with: NotRequired[Action]

    after: NotRequired[Steps]


type BuffInstead = Annotated[BuffInsteadFields, AfterValidator(_replacement)]


_LifeInsteadKeys = TypedDict("_LifeInsteadKeys", {"if": Condition})


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "anyOf": [{"required": ["delta"]}, {"required": ["replace_with"]}],
            "not": {"anyOf": [{"required": ["replace_with", "delta"]}]},
        },
    )
)
class LifeInsteadFields(_LifeInsteadKeys):
    delta: NotRequired[RecoveryDelta]

    replace_with: NotRequired[Action]

    after: NotRequired[Steps]


type LifeInstead = Annotated[LifeInsteadFields, AfterValidator(_replacement)]


_GrantInsteadKeys = TypedDict("_GrantInsteadKeys", {"if": Condition})


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "anyOf": [{"required": ["keywords"]}, {"required": ["replace_with"]}],
            "not": {"anyOf": [{"required": ["replace_with", "keywords"]}]},
        },
    )
)
class GrantInsteadFields(_GrantInsteadKeys):
    keywords: NotRequired[Annotated[tuple[Text | ParamRef, ...], Field(min_length=1)]]

    replace_with: NotRequired[Action]

    after: NotRequired[Steps]


type GrantInstead = Annotated[GrantInsteadFields, AfterValidator(_replacement)]


_CreateInsteadKeys = TypedDict("_CreateInsteadKeys", {"if": Condition})


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "anyOf": [
                {"required": ["n"]},
                {"required": ["upto"]},
                {"required": ["to"]},
                {"required": ["replace_with"]},
            ],
            "not": {
                "anyOf": [
                    {"required": ["replace_with", "n"]},
                    {"required": ["replace_with", "upto"]},
                    {"required": ["replace_with", "to"]},
                    {"required": ["n", "upto"]},
                ]
            },
        },
    )
)
class CreateInsteadFields(_CreateInsteadKeys):
    n: NotRequired[CountAmount]

    upto: NotRequired[Quantity]

    to: NotRequired[CreateDestination]

    replace_with: NotRequired[Action]

    after: NotRequired[Steps]


type CreateInstead = Annotated[CreateInsteadFields, AfterValidator(_replacement)]


_DrawInsteadKeys = TypedDict("_DrawInsteadKeys", {"if": Condition})


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "anyOf": [{"required": ["n"]}, {"required": ["replace_with"]}],
            "not": {"anyOf": [{"required": ["replace_with", "n"]}]},
        },
    )
)
class DrawInsteadFields(_DrawInsteadKeys):
    n: NotRequired[CountAmount]

    replace_with: NotRequired[Action]

    after: NotRequired[Steps]


type DrawInstead = Annotated[DrawInsteadFields, AfterValidator(_replacement)]


_SearchInsteadKeys = TypedDict("_SearchInsteadKeys", {"if": Condition})


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "anyOf": [
                {"required": ["sel"]},
                {"required": ["n"]},
                {"required": ["upto"]},
                {"required": ["to"]},
                {"required": ["replace_with"]},
            ],
            "not": {
                "anyOf": [
                    {"required": ["replace_with", "sel"]},
                    {"required": ["replace_with", "n"]},
                    {"required": ["replace_with", "upto"]},
                    {"required": ["replace_with", "to"]},
                    {"required": ["n", "upto"]},
                ]
            },
        },
    )
)
class SearchInsteadFields(_SearchInsteadKeys):
    sel: NotRequired[ObjectSel]

    n: NotRequired[SelectionQuantity]

    upto: NotRequired[SelectionQuantity]

    to: NotRequired[GroupedSearchDestination]

    replace_with: NotRequired[Action]

    after: NotRequired[Steps]


type SearchInstead = Annotated[SearchInsteadFields, AfterValidator(_replacement)]


_BanishInsteadKeys = TypedDict("_BanishInsteadKeys", {"if": Condition})


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "anyOf": [{"required": ["ref"]}, {"required": ["replace_with"]}],
            "not": {"anyOf": [{"required": ["replace_with", "ref"]}]},
        },
    )
)
class BanishInsteadFields(_BanishInsteadKeys):
    ref: NotRequired[EffectObject]

    replace_with: NotRequired[Action]

    after: NotRequired[Steps]


type BanishActionInstead = Annotated[BanishInsteadFields, AfterValidator(_replacement)]


_DestroyInsteadKeys = TypedDict("_DestroyInsteadKeys", {"if": Condition})


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "anyOf": [{"required": ["ref"]}, {"required": ["replace_with"]}],
            "not": {"anyOf": [{"required": ["replace_with", "ref"]}]},
        },
    )
)
class DestroyInsteadFields(_DestroyInsteadKeys):
    ref: NotRequired[EffectObject]

    replace_with: NotRequired[Action]

    after: NotRequired[Steps]


type DestroyInstead = Annotated[DestroyInsteadFields, AfterValidator(_replacement)]


@with_config(CONFIG)
class SearchExactFields(TypedDict):
    sel: ObjectSel

    n: SelectionQuantity

    to: SearchDestination

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    cost_sum_le: NotRequired[Quantity]

    distinct_by: NotRequired[DistinctBy]

    entry_effects: NotRequired[Annotated[tuple[EffectSpec, ...], Field(min_length=1)]]

    instead: NotRequired[Annotated[tuple[SearchInstead, ...], Field(min_length=1)]]


type SearchExact = SearchExactFields


@with_config(CONFIG)
class SearchUpToFields(TypedDict):
    sel: ObjectSel

    upto: SelectionQuantity

    to: SearchDestination

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    cost_sum_le: NotRequired[Quantity]

    distinct_by: NotRequired[DistinctBy]

    entry_effects: NotRequired[Annotated[tuple[EffectSpec, ...], Field(min_length=1)]]

    instead: NotRequired[Annotated[tuple[SearchInstead, ...], Field(min_length=1)]]


type SearchUpTo = SearchUpToFields


@with_config(CONFIG)
class SearchGroupedFields(TypedDict):
    groups: Annotated[tuple[SearchGroup, ...], Field(min_length=2)]

    to: GroupedSearchDestination

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    cost_sum_le: NotRequired[Quantity]

    distinct_by: NotRequired[DistinctBy]

    entry_effects: NotRequired[Annotated[tuple[EffectSpec, ...], Field(min_length=1)]]

    instead: NotRequired[Annotated[tuple[SearchInstead, ...], Field(min_length=1)]]


type SearchGrouped = SearchGroupedFields


type Search = SearchExact | SearchUpTo | SearchGrouped


@with_config(CONFIG)
class DrawActionFields(TypedDict):
    n: CountAmount

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    who: NotRequired[EffectPlayers | ParamRef]

    instead: NotRequired[Annotated[tuple[DrawInstead, ...], Field(min_length=1)]]


type DrawAction = DrawActionFields


@with_config(CONFIG)
class DiscardExactFields(TypedDict):
    n: CountAmount

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    who: NotRequired[EffectPlayers]

    sel: NotRequired[ObjectSel]

    random: NotRequired[bool]


type DiscardExact = DiscardExactFields


@with_config(CONFIG)
class DiscardAllFields(TypedDict):
    all: TrueFlag

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    who: NotRequired[EffectPlayers]


type DiscardAll = DiscardAllFields


@with_config(CONFIG)
class DiscardDownToFields(TypedDict):
    down_to: Quantity

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    who: NotRequired[EffectPlayers]


type DiscardDownTo = DiscardDownToFields


@with_config(CONFIG)
class DiscardAnyFields(TypedDict):
    any: TrueFlag

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    who: NotRequired[EffectPlayers]

    sel: NotRequired[ObjectSel]


type DiscardAny = DiscardAnyFields


@with_config(CONFIG)
class DiscardRefFields(TypedDict):
    ref: Ref

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    who: NotRequired[EffectPlayers]


type DiscardReferenceAction = DiscardRefFields


type DiscardAction = (
    DiscardExact | DiscardAll | DiscardDownTo | DiscardAny | DiscardReferenceAction
)


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={"dependentRequired": {"distribute_at": ["distribute"]}},
    )
)
class DamageActionFields(TypedDict):
    to: EffectRecipient

    n: DamageAmount

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    distribute: NotRequired[TrueFlag]

    distribute_at: NotRequired[Literal["play", "resolution"]]

    instead: NotRequired[Annotated[tuple[DamageInstead, ...], Field(min_length=1)]]


type DamageAction = Annotated[DamageActionFields, AfterValidator(_damage)]


@with_config(CONFIG)
class DestroyLongFields(TypedDict):
    ref: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    instead: NotRequired[Annotated[tuple[DestroyInstead, ...], Field(min_length=1)]]


type DestroyLong = DestroyLongFields


type DestroyAction = Ref | DestroyLong


@with_config(CONFIG)
class BanishLongFields(TypedDict):
    ref: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    instead: NotRequired[
        Annotated[tuple[BanishActionInstead, ...], Field(min_length=1)]
    ]


type BanishLong = BanishLongFields


@with_config(CONFIG)
class BanishTopFields(TypedDict):
    top: DeckSlice

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    instead: NotRequired[
        Annotated[tuple[BanishActionInstead, ...], Field(min_length=1)]
    ]


type BanishTopAction = BanishTopFields


type BanishAction = Ref | BanishLong | BanishTopAction


@with_config(CONFIG)
class ActLongFields(TypedDict):
    ref: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type ActLong = ActLongFields


type ActAction = Ref | ActLong


@with_config(CONFIG)
class StandLongFields(TypedDict):
    ref: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type StandLong = StandLongFields


type StandAction = Ref | StandLong


@with_config(CONFIG)
class FlipDownLongFields(TypedDict):
    ref: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type FlipDownLong = FlipDownLongFields


type FlipDownAction = Ref | FlipDownLong


@with_config(CONFIG)
class FlipUpLongFields(TypedDict):
    ref: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type FlipUpLong = FlipUpLongFields


type FlipUpAction = Ref | FlipUpLong


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "allOf": [
                {
                    "if": {
                        "properties": {"to": {"type": "string"}},
                        "required": ["to"],
                    },
                    "then": {
                        "allOf": [
                            {
                                "if": {
                                    "anyOf": [
                                        {"required": ["position"]},
                                        {"required": ["order"]},
                                    ]
                                },
                                "then": {"properties": {"to": {"const": "deck"}}},
                            },
                            {
                                "if": {"required": ["acted"]},
                                "then": {"properties": {"to": {"const": "field"}}},
                            },
                        ]
                    },
                }
            ],
            "dependentRequired": {"order": ["position"]},
        },
    )
)
class MoveRefFields(TypedDict):
    ref: EffectObject

    to: Zone | ParamRef

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    position: NotRequired[Position]

    order: NotRequired[MoveOrder]

    reveal: NotRequired[bool]

    by: NotRequired[PlayerRef]

    acted: NotRequired[bool]

    entry_effects: NotRequired[Annotated[tuple[EffectSpec, ...], Field(min_length=1)]]

    instead: NotRequired[Annotated[tuple[MoveInstead, ...], Field(min_length=1)]]


type MoveRef = Annotated[MoveRefFields, AfterValidator(_move)]


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "allOf": [
                {
                    "if": {
                        "properties": {"to": {"type": "string"}},
                        "required": ["to"],
                    },
                    "then": {
                        "allOf": [
                            {
                                "if": {
                                    "anyOf": [
                                        {"required": ["position"]},
                                        {"required": ["order"]},
                                    ]
                                },
                                "then": {"properties": {"to": {"const": "deck"}}},
                            },
                            {
                                "if": {"required": ["acted"]},
                                "then": {"properties": {"to": {"const": "field"}}},
                            },
                        ]
                    },
                }
            ],
            "dependentRequired": {"order": ["position"]},
        },
    )
)
class MoveTopFields(TypedDict):
    top: DeckSlice

    to: Zone | ParamRef

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    position: NotRequired[Position]

    order: NotRequired[MoveOrder]

    reveal: NotRequired[bool]

    by: NotRequired[PlayerRef]

    acted: NotRequired[bool]

    entry_effects: NotRequired[Annotated[tuple[EffectSpec, ...], Field(min_length=1)]]

    instead: NotRequired[Annotated[tuple[MoveInstead, ...], Field(min_length=1)]]


type MoveTop = Annotated[MoveTopFields, AfterValidator(_move)]


type MoveAction = MoveRef | MoveTop


@with_config(CONFIG)
class RandomSampleFields(TypedDict):
    n: Quantity


type RandomSample = RandomSampleFields


@with_config(CONFIG)
class LookRefFields(TypedDict):
    ref: EffectObject

    bind: BindingName

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    by: NotRequired[PlayerRef]

    random: NotRequired[RandomSample]


type LookRef = LookRefFields


@with_config(CONFIG)
class LookTopFields(TypedDict):
    top: DeckSlice

    bind: BindingName

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    by: NotRequired[PlayerRef]


type LookTop = LookTopFields


type LookAction = LookRef | LookTop


@with_config(CONFIG)
class RevealRefFields(TypedDict):
    ref: EffectObject

    bind: BindingName

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    by: NotRequired[PlayerRef]

    random: NotRequired[RandomSample]


type RevealRef = RevealRefFields


@with_config(CONFIG)
class RevealTopFields(TypedDict):
    top: DeckSlice

    bind: BindingName

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    by: NotRequired[PlayerRef]


type RevealTop = RevealTopFields


type RevealAction = RevealRef | RevealTop


@with_config(CONFIG)
class RevealUntilActionFields(TypedDict):
    sel: ObjectSel

    n: Quantity

    bind: BindingName

    matched: BindingName

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type RevealUntilAction = RevealUntilActionFields


@with_config(CONFIG)
class LifeActionFields(TypedDict):
    who: EffectPlayers

    delta: RecoveryDelta

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    instead: NotRequired[Annotated[tuple[LifeInstead, ...], Field(min_length=1)]]


type LifeAction = LifeActionFields


@with_config(CONFIG)
class SetLifeActionFields(TypedDict):
    who: EffectPlayers

    n: Quantity

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type SetLifeAction = SetLifeActionFields


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={"anyOf": [{"required": ["power"]}, {"required": ["hp"]}]},
    )
)
class BuffActionFields(TypedDict):
    on: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    power: NotRequired[NumericExpr]

    hp: NotRequired[NumericExpr]

    until: NotRequired[Until]

    instead: NotRequired[Annotated[tuple[BuffInstead, ...], Field(min_length=1)]]


type BuffAction = Annotated[BuffActionFields, AfterValidator(_stats)]


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={"anyOf": [{"required": ["power"]}, {"required": ["hp"]}]},
    )
)
class SetStatActionFields(TypedDict):
    on: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    power: NotRequired[Quantity]

    hp: NotRequired[Quantity]

    until: NotRequired[Until]


type SetStatAction = Annotated[SetStatActionFields, AfterValidator(_stats)]


@with_config(CONFIG)
class GrantActionFields(TypedDict):
    on: EffectObject

    keywords: Annotated[tuple[Text | ParamRef, ...], Field(min_length=1)]

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    until: NotRequired[Until]

    instead: NotRequired[Annotated[tuple[GrantInstead, ...], Field(min_length=1)]]


type GrantAction = GrantActionFields


@with_config(CONFIG)
class LoseAbilitiesActionFields(TypedDict):
    on: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    until: NotRequired[Until]


type LoseAbilitiesAction = LoseAbilitiesActionFields


@with_config(CONFIG)
class CounterActionFields(TypedDict):
    on: EffectObject

    name: CounterName

    delta: NumericExpr

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type CounterAction = CounterActionFields


_MoveCountersActionKeys = TypedDict("_MoveCountersActionKeys", {"from": EffectObject})


@with_config(CONFIG)
class MoveCountersActionFields(_MoveCountersActionKeys):
    to: EffectObject

    name: CounterName

    all: TrueFlag

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type MoveCountersAction = MoveCountersActionFields


@with_config(CONFIG)
class PpMaxActionFields(TypedDict):
    who: EffectPlayers

    delta: NumericExpr

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type PpMaxAction = PpMaxActionFields


@with_config(CONFIG)
class EpActionFields(TypedDict):
    who: EffectPlayers

    delta: NumericExpr

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type EpAction = EpActionFields


@with_config(CONFIG)
class SepActionFields(TypedDict):
    who: EffectPlayers

    delta: NumericExpr

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type SepAction = SepActionFields


@with_config(CONFIG)
class PpDeltaFields(TypedDict):
    who: EffectPlayers

    delta: NumericExpr

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type PpDelta = PpDeltaFields


@with_config(CONFIG)
class PpToMaxFields(TypedDict):
    who: EffectPlayers

    to_max: TrueFlag

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type PpToMax = PpToMaxFields


type PpAction = PpDelta | PpToMax


@with_config(CONFIG)
class ShuffleActionFields(TypedDict):
    who: EffectPlayers

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type ShuffleAction = ShuffleActionFields


@with_config(CONFIG)
class DiceActionFields(TypedDict):
    bind: BindingName

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    who: NotRequired[PlayerRef]


type DiceAction = DiceActionFields


@with_config(CONFIG)
class DeclareNumberFields(TypedDict):
    bind: BindingName

    kind: Literal["number"]

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    who: NotRequired[PlayerRef]

    min: NotRequired[Quantity]

    max: NotRequired[Quantity]


type DeclareNumber = DeclareNumberFields


@with_config(CONFIG)
class DeclareNameFields(TypedDict):
    bind: BindingName

    kind: Literal["name"]

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    who: NotRequired[PlayerRef]


type DeclareName = DeclareNameFields


type DeclareAction = DeclareNumber | DeclareName


@with_config(CONFIG)
class EquipActionFields(TypedDict):
    on: EffectObject

    token: Text

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type EquipAction = EquipActionFields


@with_config(CONFIG)
class StealActionFields(TypedDict):
    ref: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    to: NotRequired[PlayerRef | Literal["other_players"]]


type StealAction = StealActionFields


@with_config(CONFIG)
class GiveActionFields(TypedDict):
    ref: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    to: NotRequired[PlayerRef | Literal["other_players"]]


type GiveAction = GiveActionFields


@with_config(CONFIG)
class BoxActionFields(TypedDict):
    ref: EffectObject

    until: Until

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type BoxAction = BoxActionFields


@with_config(CONFIG)
class RaceActionFields(TypedDict):
    ref: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    n: NotRequired[Quantity]


type RaceAction = RaceActionFields


@with_config(CONFIG)
class GainDriveActionFields(TypedDict):
    ref: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type GainDriveAction = GainDriveActionFields


@with_config(CONFIG)
class PlayCardActionFields(TypedDict):
    ref: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type PlayCardAction = PlayCardActionFields


@with_config(CONFIG)
class UbInvocationFields(TypedDict):
    ub: TrueFlag

    n: Annotated[int, Field(strict=True, ge=1, le=9007199254740991)]


type UbInvocation = UbInvocationFields


@with_config(CONFIG)
class PlayAbilityTriggerFields(TypedDict):
    of: EffectObject

    trigger: TriggerKind

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    cost_policy: NotRequired[Literal["normal", "waive_original"]]


type PlayAbilityTrigger = PlayAbilityTriggerFields


@with_config(CONFIG)
class PlayAbilityAbilityFields(TypedDict):
    of: EffectObject

    ability: UbInvocation

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    cost_policy: NotRequired[Literal["normal", "waive_original"]]


type PlayAbilityAbility = PlayAbilityAbilityFields


type PlayAbilityAction = PlayAbilityTrigger | PlayAbilityAbility


@with_config(CONFIG)
class ApplyActionFields(TypedDict):
    on: EffectObject

    effect: EffectSpec

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    until: NotRequired[Until]

    capture: NotRequired[Literal["play"]]


type ApplyAction = ApplyActionFields


@with_config(CONFIG)
class PlayerEffectActionFields(TypedDict):
    who: EffectPlayers

    effect: EffectSpec

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    until: NotRequired[Until]


type PlayerEffectAction = PlayerEffectActionFields


@with_config(CONFIG)
class WinActionFields(TypedDict):
    who: EffectPlayers

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type WinAction = WinActionFields


@with_config(CONFIG)
class SkipTurnActionFields(TypedDict):
    who: EffectPlayers

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type SkipTurnAction = SkipTurnActionFields


@with_config(CONFIG)
class ExtraTurnActionFields(TypedDict):
    who: EffectPlayers

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]


type ExtraTurnAction = ExtraTurnActionFields


@with_config(CONFIG)
class MakeTypeActionFields(TypedDict):
    on: EffectObject

    type: Code

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    until: NotRequired[Until]


type MakeTypeAction = MakeTypeActionFields


@with_config(CONFIG)
class TokenAllocationFields(TypedDict):
    among: Annotated[tuple[Text, ...], Field(min_length=1)]

    total: Quantity

    by: NotRequired[PlayerRef]


type TokenAllocation = TokenAllocationFields


@with_config(CONFIG)
class AllocatedTokensFields(TypedDict):
    allocate: TokenAllocation


type AllocatedTokens = AllocatedTokensFields


type CreatedToken = (
    Text
    | ParamRef
    | Annotated[tuple[Text | ParamRef, ...], Field(min_length=1)]
    | AllocatedTokens
)


@with_config(CONFIG)
class CreateDestinationChoiceFields(TypedDict):
    one_of: Annotated[tuple[Zone, ...], Field(min_length=2)]

    by: PlayerRef

    scope: Literal["each", "batch"]


type CreateDestinationChoice = CreateDestinationChoiceFields


type CreateDestination = Zone | ParamRef | CreateDestinationChoice


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "not": {
                "anyOf": [
                    {"required": ["n", "upto"]},
                    {"required": ["n", "any"]},
                    {"required": ["upto", "any"]},
                ]
            },
            "allOf": [
                {
                    "if": {
                        "properties": {
                            "token": {"type": "object", "required": ["allocate"]}
                        },
                        "required": ["token"],
                    },
                    "then": {
                        "not": {
                            "anyOf": [
                                {"required": ["n"]},
                                {"required": ["upto"]},
                                {"required": ["any"]},
                            ]
                        }
                    },
                },
                {
                    "if": {
                        "properties": {"to": {"type": "string"}},
                        "required": ["to", "acted"],
                    },
                    "then": {"properties": {"to": {"const": "field"}}},
                },
            ],
        },
    )
)
class CreateActionFields(TypedDict):
    token: CreatedToken

    to: CreateDestination

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    n: NotRequired[CountAmount]

    upto: NotRequired[Quantity]

    any: NotRequired[TrueFlag]

    counters: NotRequired[Annotated[dict[CounterName, Quantity], Field(min_length=1)]]

    by: NotRequired[PlayerRef]

    acted: NotRequired[bool]

    entry_effects: NotRequired[Annotated[tuple[EffectSpec, ...], Field(min_length=1)]]

    bind: NotRequired[BindingName]

    instead: NotRequired[Annotated[tuple[CreateInstead, ...], Field(min_length=1)]]


type CreateAction = Annotated[CreateActionFields, AfterValidator(_create)]


@with_config(CONFIG)
class EvolveActionFields(TypedDict):
    ref: EffectObject

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    into: NotRequired[
        Text | Annotated[tuple[Text, ...], Field(min_length=1)] | ObjectSel
    ]


type EvolveAction = EvolveActionFields


@with_config(CONFIG)
class TransformAllocationFields(TypedDict):
    among: Annotated[tuple[Text, ...], Field(min_length=1)]

    each_exactly_once: TrueFlag


type TransformAllocation = TransformAllocationFields


@with_config(CONFIG)
class TransformAllocatedFields(TypedDict):
    allocate: TransformAllocation


type TransformAllocated = TransformAllocatedFields


@with_config(CONFIG)
class TransformActionFields(TypedDict):
    ref: EffectObject

    into: Text | TransformAllocated

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    entry_effects: NotRequired[Annotated[tuple[EffectSpec, ...], Field(min_length=1)]]


type TransformAction = TransformActionFields


@with_config(CONFIG)
class OptionalControlFields(TypedDict):
    do: Annotated[tuple[Step, ...], Field(min_length=1)]

    who: NotRequired[PlayerRef]

    then: NotRequired[Steps]

    result: NotRequired[BindingName]


type OptionalControl = OptionalControlFields


@with_config(CONFIG)
class PayControlFields(TypedDict):
    cost: Cost

    do: Steps

    who: NotRequired[PlayerRef]

    then: NotRequired[Steps]

    result: NotRequired[BindingName]


type PayControl = PayControlFields


_ChoiceInsteadExactKeys = TypedDict("_ChoiceInsteadExactKeys", {"if": Condition})


@with_config(CONFIG)
class ChoiceInsteadExactFields(_ChoiceInsteadExactKeys):
    n: Quantity


type ChoiceInsteadExact = ChoiceInsteadExactFields


_ChoiceInsteadUpToKeys = TypedDict("_ChoiceInsteadUpToKeys", {"if": Condition})


@with_config(CONFIG)
class ChoiceInsteadUpToFields(_ChoiceInsteadUpToKeys):
    upto: Quantity


type ChoiceInsteadUpTo = ChoiceInsteadUpToFields


type ChoiceInstead = ChoiceInsteadExact | ChoiceInsteadUpTo


@with_config(CONFIG)
class ChoiceExactFields(TypedDict):
    n: Quantity

    options: Annotated[tuple[Steps, ...], Field(min_length=1)]

    who: NotRequired[PlayerRef]

    exclude_chosen: NotRequired[Literal["this_turn"]]

    instead: NotRequired[Annotated[tuple[ChoiceInstead, ...], Field(min_length=1)]]


type ChoiceExact = ChoiceExactFields


@with_config(CONFIG)
class ChoiceUpToFields(TypedDict):
    upto: Quantity

    options: Annotated[tuple[Steps, ...], Field(min_length=1)]

    who: NotRequired[PlayerRef]

    exclude_chosen: NotRequired[Literal["this_turn"]]

    instead: NotRequired[Annotated[tuple[ChoiceInstead, ...], Field(min_length=1)]]


type ChoiceUpTo = ChoiceUpToFields


type ChoiceControl = ChoiceExact | ChoiceUpTo


@with_config(CONFIG)
class EitherControlFields(TypedDict):
    options: Annotated[tuple[Steps, ...], Field(min_length=2)]

    who: NotRequired[PlayerRef]

    bind: NotRequired[BindingName]


type EitherControl = EitherControlFields


_IfControlKeys = TypedDict("_IfControlKeys", {"else": "NotRequired[Steps]"})


@with_config(CONFIG)
class IfControlFields(_IfControlKeys):
    cond: Condition

    then: Steps


type IfControl = IfControlFields


@with_config(CONFIG)
class RepeatCollectionFields(TypedDict):
    receipts: BindingName


type RepeatCollection = RepeatCollectionFields


@with_config(CONFIG)
class RepeatCountFields(TypedDict):
    n: Quantity

    do: Steps

    collect: NotRequired[RepeatCollection]


type RepeatCount = RepeatCountFields


@with_config(CONFIG)
class RepeatUntilFields(TypedDict):
    until: Condition

    do: Steps

    collect: NotRequired[RepeatCollection]

    check: NotRequired[Literal["before", "after"]]


type RepeatUntil = RepeatUntilFields


_RepeatWhileKeys = TypedDict("_RepeatWhileKeys", {"while": Condition})


@with_config(CONFIG)
class RepeatWhileFields(_RepeatWhileKeys):
    do: Steps

    collect: NotRequired[RepeatCollection]

    check: NotRequired[Literal["before", "after"]]


type RepeatWhile = RepeatWhileFields


type RepeatControl = RepeatCount | RepeatUntil | RepeatWhile


_RepeatAbilityControlKeys = TypedDict("_RepeatAbilityControlKeys", {"while": Condition})


@with_config(CONFIG)
class RepeatAbilityControlFields(_RepeatAbilityControlKeys):
    pass


type RepeatAbilityControl = RepeatAbilityControlFields


_ForeachControlKeys = TypedDict(
    "_ForeachControlKeys", {"in": EffectObject, "as": BindingName}
)


@with_config(CONFIG)
class ForeachControlFields(_ForeachControlKeys):
    do: Steps


type ForeachControl = ForeachControlFields


@with_config(CONFIG)
class PlayerStageModeFields(TypedDict):
    do: Steps

    mode: Literal["choice", "nonchoice"]


type PlayerStageMode = PlayerStageModeFields


@with_config(CONFIG)
class PlayerStageExplicitFields(TypedDict):
    do: Steps

    mode: Literal["explicit"]

    evidence: Text


type PlayerStageExplicit = PlayerStageExplicitFields


type PlayerStage = Steps | PlayerStageMode | PlayerStageExplicit


_EachPlayerControlKeys = TypedDict(
    "_EachPlayerControlKeys", {"as": NotRequired[BindingName]}
)


@with_config(CONFIG)
class EachPlayerControlFields(_EachPlayerControlKeys):
    stages: Annotated[tuple[PlayerStage, ...], Field(min_length=1)]

    who: NotRequired[Literal["each"]]


type EachPlayerControl = EachPlayerControlFields


@with_config(CONFIG)
class SimultaneousControlFields(TypedDict):
    do: Annotated[tuple[Step, ...], Field(min_length=1)]

    result: NotRequired[BindingName]


type SimultaneousControl = SimultaneousControlFields


_ByKindControlKeys = TypedDict("_ByKindControlKeys", {"as": NotRequired[BindingName]})


@with_config(CONFIG)
class ByKindControlFields(_ByKindControlKeys):
    ref: EffectRecipient

    leader: Steps

    follower: Steps


type ByKindControl = ByKindControlFields


@with_config(
    ConfigDict(**CONFIG, json_schema_extra={"minProperties": 1, "maxProperties": 1})
)
class ActionFields(TypedDict):
    choose: NotRequired[Choose]

    search: NotRequired[Search]

    draw: NotRequired[DrawAction]

    discard: NotRequired[DiscardAction]

    damage: NotRequired[DamageAction]

    destroy: NotRequired[DestroyAction]

    banish: NotRequired[BanishAction]

    act: NotRequired[ActAction]

    stand: NotRequired[StandAction]

    flip_down: NotRequired[FlipDownAction]

    flip_up: NotRequired[FlipUpAction]

    move: NotRequired[MoveAction]

    look: NotRequired[LookAction]

    reveal: NotRequired[RevealAction]

    reveal_until: NotRequired[RevealUntilAction]

    life: NotRequired[LifeAction]

    set_life: NotRequired[SetLifeAction]

    buff: NotRequired[BuffAction]

    set_stat: NotRequired[SetStatAction]

    grant: NotRequired[GrantAction]

    lose_abilities: NotRequired[LoseAbilitiesAction]

    counter: NotRequired[CounterAction]

    move_counters: NotRequired[MoveCountersAction]

    pp_max: NotRequired[PpMaxAction]

    ep: NotRequired[EpAction]

    sep: NotRequired[SepAction]

    pp: NotRequired[PpAction]

    shuffle: NotRequired[ShuffleAction]

    dice: NotRequired[DiceAction]

    declare: NotRequired[DeclareAction]

    equip: NotRequired[EquipAction]

    steal: NotRequired[StealAction]

    give: NotRequired[GiveAction]

    stack_plus: NotRequired[NumericExpr]

    box: NotRequired[BoxAction]

    race: NotRequired[RaceAction]

    gain_drive: NotRequired[GainDriveAction]

    play_card: NotRequired[PlayCardAction]

    play_ability: NotRequired[PlayAbilityAction]

    apply: NotRequired[ApplyAction]

    player_effect: NotRequired[PlayerEffectAction]

    win: NotRequired[WinAction]

    skip_turn: NotRequired[SkipTurnAction]

    extra_turn: NotRequired[ExtraTurnAction]

    make_type: NotRequired[MakeTypeAction]

    create: NotRequired[CreateAction]

    evolve: NotRequired[EvolveAction]

    transform: NotRequired[TransformAction]


type Action = Annotated[ActionFields, AfterValidator(_single)]


_StepKeys = TypedDict("_StepKeys", {"if": NotRequired[IfControl]})


@with_config(
    ConfigDict(**CONFIG, json_schema_extra={"minProperties": 1, "maxProperties": 1})
)
class StepFields(_StepKeys):
    choose: NotRequired[Choose]

    search: NotRequired[Search]

    draw: NotRequired[DrawAction]

    discard: NotRequired[DiscardAction]

    damage: NotRequired[DamageAction]

    destroy: NotRequired[DestroyAction]

    banish: NotRequired[BanishAction]

    act: NotRequired[ActAction]

    stand: NotRequired[StandAction]

    flip_down: NotRequired[FlipDownAction]

    flip_up: NotRequired[FlipUpAction]

    move: NotRequired[MoveAction]

    look: NotRequired[LookAction]

    reveal: NotRequired[RevealAction]

    reveal_until: NotRequired[RevealUntilAction]

    life: NotRequired[LifeAction]

    set_life: NotRequired[SetLifeAction]

    buff: NotRequired[BuffAction]

    set_stat: NotRequired[SetStatAction]

    grant: NotRequired[GrantAction]

    lose_abilities: NotRequired[LoseAbilitiesAction]

    counter: NotRequired[CounterAction]

    move_counters: NotRequired[MoveCountersAction]

    pp_max: NotRequired[PpMaxAction]

    ep: NotRequired[EpAction]

    sep: NotRequired[SepAction]

    pp: NotRequired[PpAction]

    shuffle: NotRequired[ShuffleAction]

    dice: NotRequired[DiceAction]

    declare: NotRequired[DeclareAction]

    equip: NotRequired[EquipAction]

    steal: NotRequired[StealAction]

    give: NotRequired[GiveAction]

    stack_plus: NotRequired[NumericExpr]

    box: NotRequired[BoxAction]

    race: NotRequired[RaceAction]

    gain_drive: NotRequired[GainDriveAction]

    play_card: NotRequired[PlayCardAction]

    play_ability: NotRequired[PlayAbilityAction]

    apply: NotRequired[ApplyAction]

    player_effect: NotRequired[PlayerEffectAction]

    win: NotRequired[WinAction]

    skip_turn: NotRequired[SkipTurnAction]

    extra_turn: NotRequired[ExtraTurnAction]

    make_type: NotRequired[MakeTypeAction]

    create: NotRequired[CreateAction]

    evolve: NotRequired[EvolveAction]

    transform: NotRequired[TransformAction]

    optional: NotRequired[OptionalControl]

    pay: NotRequired[PayControl]

    choice: NotRequired[ChoiceControl]

    either: NotRequired[EitherControl]

    repeat: NotRequired[RepeatControl]

    repeat_ability: NotRequired[RepeatAbilityControl]

    foreach: NotRequired[ForeachControl]

    each_player: NotRequired[EachPlayerControl]

    simultaneous: NotRequired[SimultaneousControl]

    by_kind: NotRequired[ByKindControl]


type Step = Annotated[StepFields, AfterValidator(_single)]


type Steps = tuple[Step, ...]
