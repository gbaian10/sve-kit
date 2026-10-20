"""Closed authored containers over the shared ability graph; not expanded IR."""

from typing import Annotated, Literal, NotRequired, TypedDict

from pydantic import BeforeValidator, ConfigDict, Field, RootModel, with_config

from sve_carddb.contracts import author_programs as programs  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict payloads at runtime.
from sve_carddb.contracts.author_evidence import (
    BodyVersion,
    FaceId,
    ImportMap,
    Meta,
    Ports,
    Positive,
    Registration,
    Sources,
    Writers,
)
from sve_carddb.contracts.author_leaves import (
    Concept,
    ParamDeclarations,
    ParamRef,
    ParamValues,
    Scope,
    TrueFlag,
)
from sve_carddb.contracts.four_layer import Code, FrameId
from sve_carddb.core.json import integer
from sve_carddb.core.models import RecordData, Text, UInt

CONFIG = ConfigDict(extra="forbid", strict=True, regex_engine="python-re")


class MacroReference(RecordData):
    frame_id: FrameId
    ability_scope: Code
    applicability_scope: Code


class GeneratedBy(RecordData):
    macro: MacroReference
    version: BodyVersion
    params: ParamValues


@with_config(CONFIG)
class UseFields(TypedDict):
    line: Positive
    section: NotRequired[UInt]
    sources: Sources
    imports: NotRequired[ImportMap]


class InlineFields(UseFields):
    ports: Ports


@with_config(CONFIG)
class FanfareAbilityFields(InlineFields):
    fanfare: programs.TriggerHead


type FanfareAbility = FanfareAbilityFields


@with_config(CONFIG)
class LastwordAbilityFields(InlineFields):
    lastword: programs.LastwordHead


type LastwordAbility = LastwordAbilityFields


@with_config(CONFIG)
class EvolveTimeAbilityFields(InlineFields):
    evolve_time: programs.TriggerHead


type EvolveTimeAbility = EvolveTimeAbilityFields


@with_config(CONFIG)
class SuperEvolveTimeAbilityFields(InlineFields):
    super_evolve_time: programs.TriggerHead


type SuperEvolveTimeAbility = SuperEvolveTimeAbilityFields


@with_config(CONFIG)
class AttackTimeAbilityFields(InlineFields):
    attack_time: programs.TriggerHead


type AttackTimeAbility = AttackTimeAbilityFields


@with_config(CONFIG)
class RaceTimeAbilityFields(InlineFields):
    race_time: programs.TriggerHead


type RaceTimeAbility = RaceTimeAbilityFields


@with_config(CONFIG)
class DriveTimeAbilityFields(InlineFields):
    drive_time: programs.TriggerHead


type DriveTimeAbility = DriveTimeAbilityFields


@with_config(CONFIG)
class AutoAbilityFields(InlineFields):
    auto: programs.AutoHead


type AutoAbility = AutoAbilityFields


@with_config(CONFIG)
class ActivateAbilityFields(InlineFields):
    activate: programs.ActivateHead


type ActivateAbility = ActivateAbilityFields


@with_config(CONFIG)
class StaticAbilityFields(InlineFields):
    static: programs.StaticHead


type StaticAbility = StaticAbilityFields


@with_config(CONFIG)
class PlayAbilityFields(InlineFields):
    play: programs.PlayHead


type PlayAbility = PlayAbilityFields


@with_config(CONFIG)
class QuickAbilityFields(InlineFields):
    quick: TrueFlag


type QuickAbility = QuickAbilityFields


@with_config(CONFIG)
class KeywordsAbilityFields(InlineFields):
    keywords: Annotated[tuple[Concept | ParamRef, ...], Field(min_length=1)]


type KeywordsAbility = KeywordsAbilityFields


@with_config(CONFIG)
class EvolveAbilityFields(InlineFields):
    evolve: programs.EvolveHead


type EvolveAbility = EvolveAbilityFields


@with_config(CONFIG)
class RideAbilityFields(InlineFields):
    ride: programs.RideHead


type RideAbility = RideAbilityFields


@with_config(CONFIG)
class FeedAbilityFields(InlineFields):
    feed: programs.FeedHead


type FeedAbility = FeedAbilityFields


@with_config(CONFIG)
class SpellAbilityFields(InlineFields):
    spell: programs.SpellHead


type SpellAbility = SpellAbilityFields


@with_config(CONFIG)
class GeneratedAbilityFields(UseFields):
    generated_by: GeneratedBy


type GeneratedAbility = GeneratedAbilityFields


type InlineAbility = (
    FanfareAbility
    | LastwordAbility
    | EvolveTimeAbility
    | SuperEvolveTimeAbility
    | AttackTimeAbility
    | RaceTimeAbility
    | DriveTimeAbility
    | AutoAbility
    | ActivateAbility
    | StaticAbility
    | PlayAbility
    | QuickAbility
    | KeywordsAbility
    | EvolveAbility
    | RideAbility
    | FeedAbility
    | SpellAbility
)


class Ability(RootModel["InlineAbility | GeneratedAbility"]):
    model_config = ConfigDict(strict=True, regex_engine="python-re")


class DeckRule(RecordData):
    deck: Literal["main", "evolve"]
    same_name_max: UInt


class AuthorFace(RecordData):
    abilities: tuple[Ability, ...]


@with_config(CONFIG)
class AuthorCardFields(TypedDict):
    meta: Meta
    note: NotRequired[Text]
    abilities: NotRequired[tuple[Ability, ...]]
    faces: NotRequired[dict[FaceId, AuthorFace]]
    deck_rule: NotRequired[DeckRule]


type AuthorCard = AuthorCardFields


class AuthorEffectSet(RecordData):
    format: Annotated[Literal[1], BeforeValidator(integer)]
    kind: Literal["author_effect_set"]
    scope: Scope
    cards: dict[Text, AuthorCard]


@with_config(CONFIG)
class CompanionFields(TypedDict):
    frame_id: FrameId
    ability_scope: Code
    body_version: BodyVersion
    dsl: Literal["1.0"]
    params: ParamDeclarations
    body: programs.AbilityHead
    ports: Ports
    written_by: Writers
    registrations: tuple[Registration, ...]
    note: NotRequired[Text]


type Companion = CompanionFields


class EffectBodyShard(RecordData):
    format: Annotated[Literal[1], BeforeValidator(integer)]
    kind: Literal["effect_body_shard"]
    scope: Literal["shared"]
    records: tuple[Companion, ...]
