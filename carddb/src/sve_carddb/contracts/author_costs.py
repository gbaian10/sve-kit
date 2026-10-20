"""Closed payment nodes preserve authored operation order and receipt ownership."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Annotated, Literal, NotRequired, TypedDict

from pydantic import AfterValidator, ConfigDict, Field, with_config

from sve_carddb.contracts.author_leaves import CounterName, TrueFlag  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_quantities import DeckSlice  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_queries import (
    Condition,
    NumericExpr,
    ObjectSel,
    Quantity,
)
from sve_carddb.contracts.author_references import BindingName, PlayerRef, Ref  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_selection_parents import DistinctBy  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.

CONFIG = ConfigDict(extra="forbid", strict=True, regex_engine="python-re")


@with_config(CONFIG)
class CounterCostFields(TypedDict):
    name: CounterName
    result: NotRequired[BindingName]


@with_config(CONFIG)
class CounterCostSelfFields(CounterCostFields):
    on: Literal["this"]


@with_config(CONFIG)
class CounterCostSelectedFields(CounterCostFields):
    on: ObjectSel
    holders: Quantity


@with_config(CONFIG)
class CounterCostSelfExactFields(CounterCostSelfFields):
    n: Quantity


@with_config(CONFIG)
class CounterCostSelfUpToFields(CounterCostSelfFields):
    upto: Quantity


@with_config(CONFIG)
class CounterCostSelfAnyFields(CounterCostSelfFields):
    any: TrueFlag


@with_config(CONFIG)
class CounterCostSelectedExactFields(CounterCostSelectedFields):
    n: Quantity


@with_config(CONFIG)
class CounterCostSelectedUpToFields(CounterCostSelectedFields):
    upto: Quantity


@with_config(CONFIG)
class CounterCostSelectedAnyFields(CounterCostSelectedFields):
    any: TrueFlag


type CounterCostSelf = CounterCostSelfExact | CounterCostSelfUpTo | CounterCostSelfAny
type CounterCostSelected = (
    CounterCostSelectedExact | CounterCostSelectedUpTo | CounterCostSelectedAny
)
type CounterCost = CounterCostSelf | CounterCostSelected

# Named adapters carry regex configuration across recursive quantity definitions.
type CounterCostSelfExact = CounterCostSelfExactFields
type CounterCostSelfUpTo = CounterCostSelfUpToFields
type CounterCostSelfAny = CounterCostSelfAnyFields
type CounterCostSelectedExact = CounterCostSelectedExactFields
type CounterCostSelectedUpTo = CounterCostSelectedUpToFields
type CounterCostSelectedAny = CounterCostSelectedAnyFields


@with_config(CONFIG)
class MaterialGroupSource(TypedDict):
    sel: ObjectSel


@with_config(CONFIG)
class MaterialGroupExactFields(MaterialGroupSource):
    n: Quantity


@with_config(CONFIG)
class MaterialGroupUpToFields(MaterialGroupSource):
    upto: Quantity


@with_config(CONFIG)
class MaterialGroupAnyFields(MaterialGroupSource):
    any: TrueFlag


type MaterialGroupExact = MaterialGroupExactFields
type MaterialGroupUpTo = MaterialGroupUpToFields
type MaterialGroupAny = MaterialGroupAnyFields
type MaterialGroup = MaterialGroupExact | MaterialGroupUpTo | MaterialGroupAny


@with_config(CONFIG)
class MaterialMetadata(TypedDict, total=False):
    result: BindingName
    distinct_by: DistinctBy
    cost_sum_ge: Quantity


@with_config(CONFIG)
class MaterialExactFields(MaterialGroupExactFields, MaterialMetadata):
    pass


@with_config(CONFIG)
class MaterialUpToFields(MaterialGroupUpToFields, MaterialMetadata):
    pass


@with_config(CONFIG)
class MaterialAnyFields(MaterialGroupAnyFields, MaterialMetadata):
    pass


@with_config(CONFIG)
class MaterialGroupsFields(MaterialMetadata):
    groups: Annotated[tuple[MaterialGroup, ...], Field(min_length=2)]


type MaterialExact = MaterialExactFields
type MaterialUpTo = MaterialUpToFields
type MaterialAny = MaterialAnyFields
type MaterialGroups = MaterialGroupsFields
type Material = MaterialExact | MaterialUpTo | MaterialAny | MaterialGroups


@with_config(CONFIG)
class RandomDiscardMaterialFields(MaterialGroupExactFields):
    random: TrueFlag
    result: NotRequired[BindingName]


@with_config(CONFIG)
class DiscardRefFields(TypedDict):
    ref: Ref
    result: NotRequired[BindingName]


@with_config(CONFIG)
class BanishTopFields(TypedDict):
    top: DeckSlice
    result: NotRequired[BindingName]


type RandomDiscardMaterial = RandomDiscardMaterialFields
type DiscardRef = DiscardRefFields
type BanishTop = BanishTopFields


@with_config(CONFIG)
class ShuffleMaterialExactFields(MaterialExactFields):
    shuffle: TrueFlag


@with_config(CONFIG)
class ShuffleMaterialUpToFields(MaterialUpToFields):
    shuffle: TrueFlag


@with_config(CONFIG)
class ShuffleMaterialAnyFields(MaterialAnyFields):
    shuffle: TrueFlag


@with_config(CONFIG)
class ShuffleMaterialGroupsFields(MaterialGroupsFields):
    shuffle: TrueFlag


type ShuffleMaterialExact = ShuffleMaterialExactFields
type ShuffleMaterialUpTo = ShuffleMaterialUpToFields
type ShuffleMaterialAny = ShuffleMaterialAnyFields
type ShuffleMaterialGroups = ShuffleMaterialGroupsFields
type ShuffleMaterial = (
    ShuffleMaterialExact
    | ShuffleMaterialUpTo
    | ShuffleMaterialAny
    | ShuffleMaterialGroups
)


@with_config(CONFIG)
class SkipTurnFields(TypedDict):
    who: PlayerRef


type SkipTurn = SkipTurnFields

_OPERATIONS = (
    "pp",
    "ep",
    "leader_life",
    "lesson",
    "earth_rite",
    "pp_max",
    "act_self",
    "self_to_cemetery",
    "banish_self",
    "act",
    "to_cemetery",
    "return_to_hand",
    "reveal",
    "flip_down",
    "flip_up",
    "to_ex",
    "to_deck_bottom",
    "discard",
    "banish",
    "to_deck",
    "counter",
    "skip_turn",
)
STEP_CONFIG = ConfigDict(
    **CONFIG,
    json_schema_extra={
        "oneOf": [{"required": [key]} for key in (*_OPERATIONS, "one_of")]
    },
)
COST_CONFIG = ConfigDict(
    **CONFIG,
    json_schema_extra={
        "oneOf": [{"required": [key]} for key in (*_OPERATIONS, "one_of", "steps")]
    },
)
ALTERNATIVE_CONFIG = ConfigDict(
    **CONFIG,
    json_schema_extra={
        "oneOf": [{"required": [key]} for key in (*_OPERATIONS, "steps")]
    },
)


@with_config(CONFIG)
class CostOperationFields(TypedDict, total=False):
    pp: Quantity
    ep: Quantity
    leader_life: Quantity
    lesson: Quantity
    earth_rite: Quantity
    pp_max: Quantity
    act_self: TrueFlag
    self_to_cemetery: TrueFlag
    banish_self: TrueFlag
    act: Material
    to_cemetery: Material
    return_to_hand: Material
    reveal: Material
    flip_down: Material
    flip_up: Material
    to_ex: Material
    to_deck_bottom: Material
    discard: Material | RandomDiscardMaterial | DiscardRef
    banish: Material | BanishTop
    to_deck: ShuffleMaterial
    counter: CounterCost
    skip_turn: SkipTurn


@with_config(STEP_CONFIG)
class CostStepFields(CostOperationFields, total=False):
    one_of: Annotated[tuple[CostAlternative, ...], Field(min_length=2)]


@with_config(COST_CONFIG)
class CostFields(CostStepFields, total=False):
    steps: Annotated[tuple[CostStep, ...], Field(min_length=2)]


@with_config(ALTERNATIVE_CONFIG)
class CostAlternativeFields(CostOperationFields, total=False):
    steps: Annotated[tuple[CostStep, ...], Field(min_length=2)]


def _validate_cost[T: Mapping[str, object]](value: T) -> T:
    if sum(key in value for key in (*_OPERATIONS, "one_of", "steps")) != 1:
        raise ValueError("A cost requires exactly one operation or ordered steps")
    return value


type CostStep = Annotated[CostStepFields, AfterValidator(_validate_cost)]
type Cost = Annotated[CostFields, AfterValidator(_validate_cost)]
type CostAlternative = Annotated[CostAlternativeFields, AfterValidator(_validate_cost)]


@with_config(COST_CONFIG)
class AdditionalCostFields(CostFields, total=False):
    result: BindingName


@with_config(COST_CONFIG)
class OptionalCostFields(AdditionalCostFields, total=False):
    option: Annotated[int, Field(ge=1, le=9007199254740991)]


@with_config(COST_CONFIG)
class OptionalModeCostFields(AdditionalCostFields):
    option: Annotated[int, Field(ge=1, le=9007199254740991)]


type AdditionalCost = Annotated[AdditionalCostFields, AfterValidator(_validate_cost)]
type OptionalCost = Annotated[OptionalCostFields, AfterValidator(_validate_cost)]
type OptionalModeCost = Annotated[
    OptionalModeCostFields, AfterValidator(_validate_cost)
]
type OptionalCosts = (
    OptionalCost | Annotated[tuple[OptionalModeCost, ...], Field(min_length=2)]
)


_AltCostCondition = TypedDict("_AltCostCondition", {"if": Condition})


@with_config(CONFIG)
class AltCostFields(_AltCostCondition):
    cost: Cost
    may: NotRequired[bool]


@with_config(CONFIG)
class FreeXFields(TypedDict, total=False):
    min: Quantity
    max: Quantity


@with_config(CONFIG)
class PaymentXFields(TypedDict):
    from_payment: BindingName


type AltCost = AltCostFields
type FreeX = FreeXFields
type PaymentX = PaymentXFields
type XParameter = FreeX | PaymentX


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "dependentRequired": {
                "optional_cost_delta": ["optional_cost"],
                "optional_cost_set": ["optional_cost"],
            },
            "if": {
                "anyOf": [
                    {"required": ["optional_cost_delta"]},
                    {"required": ["optional_cost_set"]},
                ]
            },
            "then": {"properties": {"optional_cost": {"type": "object"}}},
        },
    )
)
class PlayCostsFields(TypedDict, total=False):
    cost_delta: NumericExpr
    cost_set: Quantity
    additional_cost: AdditionalCost
    optional_cost: OptionalCosts
    optional_cost_delta: NumericExpr
    optional_cost_set: Quantity
    alt_cost: AltCost


def _validate_play_costs(value: PlayCostsFields) -> PlayCostsFields:
    if ("optional_cost_delta" in value or "optional_cost_set" in value) and (
        "optional_cost" not in value or isinstance(value["optional_cost"], tuple)
    ):
        raise ValueError("Optional cost modifiers require one optional payment")
    return value


type PlayCosts = Annotated[PlayCostsFields, AfterValidator(_validate_play_costs)]
