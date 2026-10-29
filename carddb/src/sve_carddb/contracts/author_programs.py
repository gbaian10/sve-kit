"""Closed ability, effect, and step graph; scope and execution admission belong to C1/D0."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Annotated, Literal, NotRequired, TypedDict

from pydantic import AfterValidator, ConfigDict, Field, RootModel, with_config

from sve_carddb.contracts.author_costs import (
    Cost,
    OptionalCosts,
    PlayCostsFields,
    XParameter,
    validate_play_costs,
)
from sve_carddb.contracts.author_leaves import (
    CardName,
    Concept,
    CounterName,
    ParamRef,
    Phase,
    TrueFlag,
    Zone,
)
from sve_carddb.contracts.author_quantities import DeckSlice, SelectionQuantity  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_queries import (
    FILTER_CONSTRAINTS,
    CollectionSource,
    Condition,
    CountFilterFields,
    EventPattern,
    NumericExpr,
    ObjectSel,
    Quantity,
    RecipientSel,
    validate_object_filters,
)
from sve_carddb.contracts.author_references import (
    BindingName,
    PlayerRef,
    PlayerSet,
    Ref,
    Until,
)
from sve_carddb.contracts.author_selection_parents import (
    DistinctBy,  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
    TargetGroupFilter,  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
    Targets,  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
)
from sve_carddb.contracts.four_layer import Code  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.core.models import Text


def _grant[T: Mapping[str, object]](value: T) -> T:
    if not ("keywords" in value or "abilities" in value):
        raise ValueError("At least one granted keyword or ability is required")
    return value


def _delayed[T: Mapping[str, object]](value: T) -> T:
    if value.get("next_only") is False and "until" not in value:
        raise ValueError("Repeated delayed triggers require an expiry")
    return value


def _play(value: PlayHeadFields) -> PlayHeadFields:
    validate_play_costs(value)
    if not value.keys() - {"define"}:
        raise ValueError("A play head requires a modifier or permission")
    return value


CONFIG = ConfigDict(extra="forbid", strict=True, regex_engine="python-re")

type EffectObject = Ref | ObjectSel
type EffectRecipient = Ref | RecipientSel
type EffectPlayers = PlayerRef | PlayerSet
TriggerKind = Literal[
    "fanfare",
    "lastword",
    "evolve_time",
    "super_evolve_time",
    "attack_time",
    "race_time",
    "drive_time",
]


def _stats[T: Mapping[str, object]](value: T) -> T:
    if not ("power" in value or "hp" in value):
        raise ValueError("At least one stat is required")
    return value


def _adjustment[T: Mapping[str, object]](value: T) -> T:
    if sum(key in value for key in ("delta", "set", "multiply")) != 1:
        raise ValueError("Exactly one adjustment is required")
    return value


def _scope[T: Mapping[str, object]](value: T) -> T:
    if "on" in value and "who" in value:
        raise ValueError("Object and player scopes are exclusive")
    return value


def _trigger_scope[T: Mapping[str, object]](value: T) -> T:
    if sum(key in value for key in ("on", "who")) != 1:
        raise ValueError("Exactly one trigger scope is required")
    return value


def _card_predicate[T: Mapping[str, object]](value: T) -> T:
    if not value:
        raise ValueError("Card predicate must not be empty")
    return validate_object_filters(value)


def _single_effect[T: Mapping[str, object]](value: T) -> T:
    if len(value) != 1:
        raise ValueError("Exactly one effect key is required")
    return value


@with_config(
    ConfigDict(
        extra="forbid",
        strict=True,
        regex_engine="python-re",
        json_schema_extra={"anyOf": [{"required": ["power"]}, {"required": ["hp"]}]},
    )
)
class StatFields(TypedDict):
    on: NotRequired[EffectObject]
    power: NotRequired[NumericExpr]
    hp: NotRequired[NumericExpr]


type Stat = Annotated[StatFields, AfterValidator(_stats)]


@with_config(
    ConfigDict(
        extra="forbid",
        strict=True,
        regex_engine="python-re",
        json_schema_extra={"anyOf": [{"required": ["power"]}, {"required": ["hp"]}]},
    )
)
class SetStatFields(TypedDict):
    on: NotRequired[EffectObject]
    power: NotRequired[Quantity]
    hp: NotRequired[Quantity]


type SetStat = Annotated[SetStatFields, AfterValidator(_stats)]


@with_config(CONFIG)
class KeywordsFields(TypedDict):
    on: NotRequired[EffectObject]
    list: Annotated[tuple[Text, ...], Field(min_length=1)]


type Keywords = KeywordsFields


@with_config(CONFIG)
class LoseAbilitiesFields(TypedDict):
    on: NotRequired[EffectObject]


type LoseAbilities = LoseAbilitiesFields


@with_config(CONFIG)
class AlsoNamedFields(TypedDict):
    on: NotRequired[EffectObject]
    name: Text


type AlsoNamed = AlsoNamedFields


@with_config(CONFIG)
class TraitEffectFields(TypedDict):
    on: NotRequired[EffectObject]
    trait: Text


type TraitEffect = TraitEffectFields


@with_config(CONFIG)
class MakeTypeFields(TypedDict):
    on: NotRequired[EffectObject]
    type: Code


type MakeType = MakeTypeFields


@with_config(
    ConfigDict(
        extra="forbid",
        strict=True,
        regex_engine="python-re",
        json_schema_extra={"oneOf": [{"required": ["delta"]}, {"required": ["set"]}]},
    )
)
class CostModFields(TypedDict):
    when_playing: ObjectSel
    delta: NotRequired[NumericExpr | ParamRef]
    set: NotRequired[Quantity | ParamRef]
    next_only: NotRequired[bool]


type CostMod = Annotated[CostModFields, AfterValidator(_adjustment)]


@with_config(
    ConfigDict(
        extra="forbid",
        strict=True,
        regex_engine="python-re",
        json_schema_extra={
            "oneOf": [{"required": ["delta"]}, {"required": ["multiply"]}]
        },
    )
)
class DamageModFields(TypedDict):
    on: NotRequired[EffectRecipient]
    source: NotRequired[EffectObject]
    kind: NotRequired[Literal["any", "attack", "combat", "ability"]]
    delta: NotRequired[NumericExpr]
    multiply: NotRequired[Quantity]
    next_only: NotRequired[bool]


type DamageMod = Annotated[DamageModFields, AfterValidator(_adjustment)]


@with_config(CONFIG)
class PreventFields(TypedDict):
    on: NotRequired[EffectRecipient]
    kind: Literal["any", "combat", "ability"]
    source: NotRequired[EffectObject]
    next_only: NotRequired[bool]


type Prevent = PreventFields


@with_config(CONFIG)
class DamageCapFields(TypedDict):
    on: NotRequired[EffectRecipient]
    threshold: Quantity
    set: Quantity


type DamageCap = DamageCapFields


@with_config(
    ConfigDict(
        extra="forbid",
        strict=True,
        regex_engine="python-re",
        json_schema_extra={"oneOf": [{"required": ["delta"]}, {"required": ["set"]}]},
    )
)
class EvolveCostModFields(TypedDict):
    on: EffectObject
    delta: NotRequired[NumericExpr]
    set: NotRequired[Quantity]


type EvolveCostMod = Annotated[EvolveCostModFields, AfterValidator(_adjustment)]


_BanishReplacementEffectKeys = TypedDict(
    "_BanishReplacementEffectKeys", {"from": 'Literal["field"]'}
)


@with_config(CONFIG)
class BanishReplacementEffectFields(_BanishReplacementEffectKeys):
    on: NotRequired[EffectObject]
    to: Literal["cemetery"]


type BanishReplacementEffect = BanishReplacementEffectFields


@with_config(CONFIG)
class ObjectEffectFields(TypedDict):
    on: EffectObject


type ObjectEffect = ObjectEffectFields


_PlayPermissionKeys = TypedDict(
    "_PlayPermissionKeys", {"from": "Annotated[tuple[Zone, ...], Field(min_length=1)]"}
)


@with_config(CONFIG)
class PlayPermissionFields(_PlayPermissionKeys):
    on: EffectObject


type PlayPermission = PlayPermissionFields


@with_config(
    ConfigDict(
        extra="forbid",
        strict=True,
        regex_engine="python-re",
        json_schema_extra={"oneOf": [{"required": ["on"]}, {"required": ["who"]}]},
    )
)
class NoTriggerFields(TypedDict):
    on: NotRequired[Ref]
    who: NotRequired[EffectPlayers]
    kinds: Annotated[tuple[TriggerKind, ...], Field(min_length=1)]


type NoTrigger = Annotated[NoTriggerFields, AfterValidator(_trigger_scope)]


@with_config(CONFIG)
class PlayerEffectFields(TypedDict):
    who: EffectPlayers


type PlayerEffect = PlayerEffectFields


@with_config(CONFIG)
class RerollFields(TypedDict):
    who: EffectPlayers
    times: Quantity


type Reroll = RerollFields


@with_config(CONFIG)
class ExtraTriggerFields(TypedDict):
    phase: Phase
    who: EffectPlayers
    n: Quantity


type ExtraTrigger = ExtraTriggerFields


@with_config(CONFIG)
class MustTargetFields(TypedDict):
    on: EffectObject
    by: EffectPlayers


type MustTarget = MustTargetFields


@with_config(CONFIG)
class SpellchainPopulationFields(TypedDict):
    rule: Literal["spellchain_population"]
    who: EffectPlayers
    of: CollectionSource


type SpellchainPopulation = SpellchainPopulationFields


@with_config(CONFIG)
class CounterCapFields(TypedDict):
    rule: Literal["counter_cap"]
    on: EffectObject
    name: CounterName
    max: Quantity


type CounterCap = CounterCapFields


@with_config(CONFIG)
class BattleDamageBasisFields(TypedDict):
    rule: Literal["battle_damage_basis"]
    on: EffectObject
    basis: Literal["hp"]


type BattleDamageBasis = BattleDamageBasisFields


type RuleOverride = SpellchainPopulation | CounterCap | BattleDamageBasis


@with_config(
    ConfigDict(
        extra="forbid",
        strict=True,
        regex_engine="python-re",
        json_schema_extra={**FILTER_CONSTRAINTS, "minProperties": 1},
    )
)
class CardPredicateFields(CountFilterFields):
    zone: NotRequired[Zone | Annotated[tuple[Zone, ...], Field(min_length=1)]]


type CardPredicate = Annotated[CardPredicateFields, AfterValidator(_card_predicate)]


@with_config(CONFIG)
class KeywordAbilityFields(TypedDict):
    keyword: TriggerKind


type KeywordAbility = KeywordAbilityFields


_ActivatedAbilityKeys = TypedDict(
    "_ActivatedAbilityKeys",
    {
        "except": 'NotRequired[Annotated[tuple[Literal["evolution"], ...], Field(min_length=1)]]'
    },
)


@with_config(CONFIG)
class ActivatedAbilityFields(_ActivatedAbilityKeys):
    kind: Literal["activated"]


type ActivatedAbility = ActivatedAbilityFields


type AbilityPredicate = KeywordAbility | ActivatedAbility


@with_config(CONFIG)
class DrawNotPhaseFields(TypedDict):
    not_phase: Phase


type DrawNotPhase = DrawNotPhaseFields


@with_config(CONFIG)
class DrawRulePhaseFields(TypedDict):
    phase: Phase
    cause: Literal["rule"]


type DrawRulePhase = DrawRulePhaseFields


@with_config(CONFIG)
class StandRulePhaseFields(TypedDict):
    phase: Literal["start"]
    who: PlayerRef
    cause: Literal["rule"]


type StandRulePhase = StandRulePhaseFields


@with_config(
    ConfigDict(
        extra="forbid",
        strict=True,
        regex_engine="python-re",
        json_schema_extra={"not": {"required": ["on", "who"]}},
    )
)
class CannotPlayCardFields(TypedDict):
    what: Literal["play_card"]
    on: NotRequired[EffectObject]
    who: NotRequired[EffectPlayers]
    card: NotRequired[CardPredicate]


type CannotPlayCard = Annotated[CannotPlayCardFields, AfterValidator(_scope)]


@with_config(
    ConfigDict(
        extra="forbid",
        strict=True,
        regex_engine="python-re",
        json_schema_extra={"not": {"required": ["on", "who"]}},
    )
)
class CannotPlayAbilityFields(TypedDict):
    what: Literal["play_ability"]
    on: NotRequired[EffectObject]
    who: NotRequired[EffectPlayers]
    ability: NotRequired[AbilityPredicate]


type CannotPlayAbility = Annotated[CannotPlayAbilityFields, AfterValidator(_scope)]


@with_config(CONFIG)
class CannotAttackFields(TypedDict):
    what: Literal["attack"]
    on: NotRequired[EffectObject]
    target: NotRequired[RecipientSel]


type CannotAttack = CannotAttackFields


@with_config(CONFIG)
class CannotChooseTargetFields(TypedDict):
    what: Literal["choose_target"]
    on: NotRequired[EffectObject]
    by: NotRequired[EffectPlayers]


type CannotChooseTarget = CannotChooseTargetFields


@with_config(CONFIG)
class CannotDrawFields(TypedDict):
    what: Literal["draw"]
    who: NotRequired[EffectPlayers]
    during: NotRequired[DrawNotPhase | DrawRulePhase]


type CannotDraw = CannotDrawFields


@with_config(CONFIG)
class CannotRemoveFields(TypedDict):
    what: Literal["destroy", "banish"]
    on: NotRequired[EffectObject]
    by: NotRequired[Literal["ability"]]


type CannotRemove = CannotRemoveFields


@with_config(CONFIG)
class CannotStandFields(TypedDict):
    what: Literal["stand"]
    on: NotRequired[EffectObject]
    during: NotRequired[StandRulePhase]


type CannotStand = CannotStandFields


@with_config(CONFIG)
class CannotPlayerFields(TypedDict):
    what: Literal["win", "lose", "pp_max_increase"]
    who: NotRequired[EffectPlayers]


type CannotPlayer = CannotPlayerFields


@with_config(CONFIG)
class CannotDealDamageFields(TypedDict):
    what: Literal["deal_damage"]
    on: NotRequired[EffectObject]


type CannotDealDamage = CannotDealDamageFields


type Cannot = (
    CannotPlayCard
    | CannotPlayAbility
    | CannotAttack
    | CannotChooseTarget
    | CannotDraw
    | CannotRemove
    | CannotStand
    | CannotPlayer
    | CannotDealDamage
)


@with_config(
    ConfigDict(
        extra="forbid",
        strict=True,
        regex_engine="python-re",
        json_schema_extra={"minProperties": 1, "maxProperties": 1},
    )
)
class EffectSpecFields(TypedDict):
    grant: NotRequired[GrantEffect]
    stat: NotRequired[Stat]
    set_stat: NotRequired[SetStat]
    keywords: NotRequired[Keywords]
    lose_abilities: NotRequired[LoseAbilities]
    also_named: NotRequired[AlsoNamed]
    add_trait: NotRequired[TraitEffect]
    remove_trait: NotRequired[TraitEffect]
    make_type: NotRequired[MakeType]
    cost_mod: NotRequired[CostMod]
    damage_mod: NotRequired[DamageMod]
    prevent: NotRequired[Prevent]
    damage_cap: NotRequired[DamageCap]
    evolve_cost_mod: NotRequired[EvolveCostMod]
    banish_instead: NotRequired[BanishReplacementEffect]
    cannot: NotRequired[Cannot]
    must_attack: NotRequired[ObjectEffect]
    play_permission: NotRequired[PlayPermission]
    no_trigger: NotRequired[NoTrigger]
    unlimited_evolve: NotRequired[PlayerEffect]
    enters_acted: NotRequired[ObjectEffect]
    reroll: NotRequired[Reroll]
    ignore_ward: NotRequired[ObjectEffect]
    choice_any: NotRequired[PlayerEffect]
    extra_trigger: NotRequired[ExtraTrigger]
    must_target: NotRequired[MustTarget]
    rule_override: NotRequired[RuleOverride]


type EffectSpec = Annotated[EffectSpecFields, AfterValidator(_single_effect)]


def _single[T: Mapping[str, object]](value: T) -> T:
    if len(value) != 1:
        raise ValueError("Exactly one operation is required")
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


_ChooseExactKeys = TypedDict("_ChooseExactKeys", {"from": RecipientSel})


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


_ChooseUpToKeys = TypedDict("_ChooseUpToKeys", {"from": RecipientSel})


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
            "anyOf": [
                {"required": ["keywords"]},
                {"required": ["abilities"]},
                {"required": ["replace_with"]},
            ],
            "not": {
                "anyOf": [
                    {"required": ["replace_with", "keywords"]},
                    {"required": ["replace_with", "abilities"]},
                ]
            },
        },
    )
)
class GrantInsteadFields(_GrantInsteadKeys):
    abilities: NotRequired[
        Annotated[tuple[GrantedAbilityHead, ...], Field(min_length=1)]
    ]
    keywords: NotRequired[
        Annotated[tuple[Concept | ParamRef, ...], Field(min_length=1)]
    ]

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


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "anyOf": [{"required": ["keywords"]}, {"required": ["abilities"]}]
        },
    )
)
class GrantActionFields(TypedDict):
    on: EffectRecipient

    keywords: NotRequired[
        Annotated[tuple[Concept | ParamRef, ...], Field(min_length=1)]
    ]
    abilities: NotRequired[
        Annotated[tuple[GrantedAbilityHead, ...], Field(min_length=1)]
    ]

    result: NotRequired[BindingName]

    precondition: NotRequired[Condition]

    until: NotRequired[Until]

    instead: NotRequired[Annotated[tuple[GrantInstead, ...], Field(min_length=1)]]


type GrantAction = Annotated[GrantActionFields, AfterValidator(_grant)]


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

    into: NotRequired[EvolutionDestination]


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
    delayed: NotRequired[DelayedAction]
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


# Nominal roots bound the type checker's alias traversal without changing JSON.
class Action(RootModel[Annotated[ActionFields, AfterValidator(_single)]]):
    model_config = ConfigDict(strict=True, regex_engine="python-re")


_StepKeys = TypedDict("_StepKeys", {"if": NotRequired[IfControl]})


@with_config(
    ConfigDict(**CONFIG, json_schema_extra={"minProperties": 1, "maxProperties": 1})
)
class StepFields(_StepKeys):
    delayed: NotRequired[DelayedAction]
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


class Step(RootModel[Annotated[StepFields, AfterValidator(_single)]]):
    model_config = ConfigDict(strict=True, regex_engine="python-re")


type Steps = tuple[Step, ...]

type AbilityZone = Zone | Literal["equipment"]


type AbilityZones = (
    AbilityZone | Annotated[tuple[AbilityZone, ...], Field(min_length=1)]
)


type Definitions = Annotated[dict[BindingName, NumericExpr], Field(min_length=1)]


type EvolutionDestination = (
    CardName | Annotated[tuple[CardName, ...], Field(min_length=1)] | ObjectSel
)


@with_config(CONFIG)
class FusionExactFields(TypedDict):
    sel: ObjectSel

    n: Quantity

    result: NotRequired[BindingName]


type FusionExact = FusionExactFields


@with_config(CONFIG)
class FusionUpToFields(TypedDict):
    sel: ObjectSel

    upto: Quantity

    result: NotRequired[BindingName]


type FusionUpTo = FusionUpToFields


@with_config(CONFIG)
class FusionAnyFields(TypedDict):
    sel: ObjectSel

    any: TrueFlag

    result: NotRequired[BindingName]


type FusionAny = FusionAnyFields


type Fusion = FusionExact | FusionUpTo | FusionAny


@with_config(CONFIG)
class TriggerHeadFields(TypedDict):
    do: Steps

    targets: NotRequired[Targets]

    optional_cost: NotRequired[OptionalCosts]

    x: NotRequired[XParameter]

    define: NotRequired[Definitions]

    per_turn: NotRequired[Annotated[int, Field(strict=True, ge=1, le=9007199254740991)]]

    ub: NotRequired[TrueFlag]

    cost: NotRequired[Cost]


type TriggerHead = TriggerHeadFields


@with_config(CONFIG)
class LastwordHeadFields(TypedDict):
    do: Steps

    targets: NotRequired[Targets]

    optional_cost: NotRequired[OptionalCosts]

    x: NotRequired[XParameter]

    define: NotRequired[Definitions]

    per_turn: NotRequired[Annotated[int, Field(strict=True, ge=1, le=9007199254740991)]]

    ub: NotRequired[TrueFlag]

    cost: NotRequired[Cost]


type LastwordHead = LastwordHeadFields


@with_config(CONFIG)
class AutoHeadFields(TypedDict):
    on: EventPattern

    do: Steps

    targets: NotRequired[Targets]

    optional_cost: NotRequired[OptionalCosts]

    x: NotRequired[XParameter]

    define: NotRequired[Definitions]

    per_turn: NotRequired[Annotated[int, Field(strict=True, ge=1, le=9007199254740991)]]

    ub: NotRequired[TrueFlag]

    zone: NotRequired[AbilityZones]

    aggregation: NotRequired[Literal["batch"]]


type AutoHead = AutoHeadFields


@with_config(CONFIG)
class ActivateCostHeadFields(TypedDict):
    cost: Cost

    do: Steps

    targets: NotRequired[Targets]

    optional_cost: NotRequired[OptionalCosts]

    x: NotRequired[XParameter]

    define: NotRequired[Definitions]

    per_turn: NotRequired[Annotated[int, Field(strict=True, ge=1, le=9007199254740991)]]

    ub: NotRequired[TrueFlag]

    zone: NotRequired[AbilityZones]

    usable_if: NotRequired[Condition]

    quick: NotRequired[TrueFlag]

    advance: NotRequired[TrueFlag]


type ActivateCostHead = ActivateCostHeadFields


@with_config(CONFIG)
class ActivateFusionHeadFields(TypedDict):
    fusion: Fusion

    do: Steps

    targets: NotRequired[Targets]

    optional_cost: NotRequired[OptionalCosts]

    x: NotRequired[XParameter]

    define: NotRequired[Definitions]

    per_turn: NotRequired[Annotated[int, Field(strict=True, ge=1, le=9007199254740991)]]

    ub: NotRequired[TrueFlag]

    zone: NotRequired[AbilityZones]

    usable_if: NotRequired[Condition]


type ActivateFusionHead = ActivateFusionHeadFields


type ActivateHead = ActivateCostHead | ActivateFusionHead


@with_config(CONFIG)
class EvolveHeadFields(TypedDict):
    cost: Cost

    into: NotRequired[EvolutionDestination]

    usable_if: NotRequired[Condition]


type EvolveHead = EvolveHeadFields


@with_config(CONFIG)
class RideHeadFields(TypedDict):
    cost: Cost


type RideHead = RideHeadFields


@with_config(CONFIG)
class FeedHeadFields(TypedDict):
    cost: Cost

    times: NotRequired[Annotated[int, Field(strict=True, ge=1, le=9007199254740991)]]


type FeedHead = FeedHeadFields


@with_config(CONFIG)
class SpellHeadFields(TypedDict):
    do: Steps

    targets: NotRequired[Targets]

    optional_cost: NotRequired[OptionalCosts]

    x: NotRequired[XParameter]

    define: NotRequired[Definitions]


type SpellHead = SpellHeadFields


_StaticHeadKeys = TypedDict("_StaticHeadKeys", {"while": NotRequired[Condition]})


@with_config(CONFIG)
class StaticHeadFields(_StaticHeadKeys):
    effects: Annotated[tuple[EffectSpec, ...], Field(min_length=1)]

    zone: NotRequired[AbilityZones]


type StaticHead = StaticHeadFields


type PlayZones = Zone | Annotated[tuple[Zone, ...], Field(min_length=1)]


_ConditionalPlayZonesKeys = TypedDict("_ConditionalPlayZonesKeys", {"if": Condition})


@with_config(CONFIG)
class ConditionalPlayZonesFields(_ConditionalPlayZonesKeys):
    zone: PlayZones


type ConditionalPlayZones = ConditionalPlayZonesFields


type PlayableFrom = PlayZones | ConditionalPlayZones


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
            "anyOf": [
                {"required": ["cost_delta"]},
                {"required": ["cost_set"]},
                {"required": ["additional_cost"]},
                {"required": ["optional_cost"]},
                {"required": ["optional_cost_delta"]},
                {"required": ["optional_cost_set"]},
                {"required": ["alt_cost"]},
                {"required": ["cannot_if"]},
                {"required": ["playable_from"]},
            ],
        },
    )
)
class PlayHeadFields(PlayCostsFields):
    cannot_if: NotRequired[Condition]
    playable_from: NotRequired[PlayableFrom]
    define: NotRequired[Definitions]


type PlayHead = Annotated[PlayHeadFields, AfterValidator(_play)]


@with_config(
    ConfigDict(**CONFIG, json_schema_extra={"minProperties": 1, "maxProperties": 1})
)
class GrantedAbilityHeadFields(TypedDict):
    fanfare: NotRequired[TriggerHead]

    lastword: NotRequired[LastwordHead]

    evolve_time: NotRequired[TriggerHead]

    super_evolve_time: NotRequired[TriggerHead]

    attack_time: NotRequired[TriggerHead]

    race_time: NotRequired[TriggerHead]

    drive_time: NotRequired[TriggerHead]

    auto: NotRequired[AutoHead]

    activate: NotRequired[ActivateHead]

    static: NotRequired[StaticHead]

    play: NotRequired[PlayHead]


class GrantedAbilityHead(
    RootModel[Annotated[GrantedAbilityHeadFields, AfterValidator(_single)]]
):
    model_config = ConfigDict(strict=True, regex_engine="python-re")


@with_config(
    ConfigDict(**CONFIG, json_schema_extra={"minProperties": 1, "maxProperties": 1})
)
class CompanionHeadFields(TypedDict):
    fanfare: NotRequired[TriggerHead]

    lastword: NotRequired[LastwordHead]

    evolve_time: NotRequired[TriggerHead]

    super_evolve_time: NotRequired[TriggerHead]

    attack_time: NotRequired[TriggerHead]

    race_time: NotRequired[TriggerHead]

    drive_time: NotRequired[TriggerHead]

    auto: NotRequired[AutoHead]

    activate: NotRequired[ActivateHead]

    static: NotRequired[StaticHead]

    play: NotRequired[PlayHead]

    evolve: NotRequired[EvolveHead]

    ride: NotRequired[RideHead]

    feed: NotRequired[FeedHead]

    spell: NotRequired[SpellHead]


type CompanionHead = Annotated[CompanionHeadFields, AfterValidator(_single)]


@with_config(
    ConfigDict(**CONFIG, json_schema_extra={"minProperties": 1, "maxProperties": 1})
)
class AbilityHeadFields(CompanionHeadFields):
    keywords: NotRequired[
        Annotated[tuple[Concept | ParamRef, ...], Field(min_length=1)]
    ]


type AbilityHead = Annotated[AbilityHeadFields, AfterValidator(_single)]


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "anyOf": [{"required": ["keywords"]}, {"required": ["abilities"]}]
        },
    )
)
class GrantEffectFields(TypedDict):
    on: NotRequired[EffectRecipient]

    keywords: NotRequired[Annotated[tuple[Concept, ...], Field(min_length=1)]]

    abilities: NotRequired[
        Annotated[tuple[GrantedAbilityHead, ...], Field(min_length=1)]
    ]


type GrantEffect = Annotated[GrantEffectFields, AfterValidator(_grant)]


@with_config(
    ConfigDict(
        **CONFIG,
        json_schema_extra={
            "if": {
                "properties": {"next_only": {"const": False}},
                "required": ["next_only"],
            },
            "then": {"required": ["until"]},
        },
    )
)
class DelayedActionFields(TypedDict):
    on: EventPattern

    do: Steps

    targets: NotRequired[Targets]

    optional_cost: NotRequired[OptionalCosts]

    x: NotRequired[XParameter]

    define: NotRequired[Definitions]

    aggregation: NotRequired[Literal["batch"]]

    next_only: NotRequired[bool]

    until: NotRequired[Until]

    precondition: NotRequired[Condition]


type DelayedAction = Annotated[DelayedActionFields, AfterValidator(_delayed)]


# Complete forward references with each root's config before adapter construction.
Action.model_rebuild()
Step.model_rebuild()
GrantedAbilityHead.model_rebuild()
