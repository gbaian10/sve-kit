"""Closed effect installation shapes; scope and execution admission belong to C1/D0."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Annotated, Literal, NotRequired, TypedDict

from pydantic import AfterValidator, ConfigDict, Field, with_config

from sve_carddb.contracts.author_leaves import CounterName, ParamRef, Phase, Zone  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_queries import (
    FILTER_CONSTRAINTS,
    CollectionSource,
    CountFilterFields,
    NumericExpr,
    ObjectSel,
    Quantity,
    RecipientSel,
    validate_object_filters,
)
from sve_carddb.contracts.author_references import PlayerRef, PlayerSet, Ref
from sve_carddb.contracts.four_layer import Code  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.core.models import Text  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.

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


_BanishInsteadKeys = TypedDict("_BanishInsteadKeys", {"from": 'Literal["field"]'})


@with_config(CONFIG)
class BanishInsteadFields(_BanishInsteadKeys):
    on: NotRequired[EffectObject]
    to: Literal["cemetery"]


type BanishInstead = BanishInsteadFields


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
    banish_instead: NotRequired[BanishInstead]
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
