"""Independent Python correspondence for the closed ability graph."""

from pydantic import TypeAdapter

from sve_carddb.contracts import author_programs as programs

from .author_selector_cases import CONFIG

ABILITIES: dict[str, TypeAdapter[object]] = {
    "A.ability-zone": TypeAdapter(programs.AbilityZone, config=CONFIG),
    "A.ability-zones": TypeAdapter(programs.AbilityZones, config=CONFIG),
    "A.define": TypeAdapter(programs.Definitions, config=CONFIG),
    "A.evolution-destination": TypeAdapter(
        programs.EvolutionDestination, config=CONFIG
    ),
    "A.fusion.exact": TypeAdapter(programs.FusionExact, config=CONFIG),
    "A.fusion.up-to": TypeAdapter(programs.FusionUpTo, config=CONFIG),
    "A.fusion.any": TypeAdapter(programs.FusionAny, config=CONFIG),
    "A.fusion": TypeAdapter(programs.Fusion, config=CONFIG),
    "A.head.trigger": TypeAdapter(programs.TriggerHead, config=CONFIG),
    "A.head.lastword": TypeAdapter(programs.LastwordHead, config=CONFIG),
    "A.head.auto": TypeAdapter(programs.AutoHead, config=CONFIG),
    "A.head.activate.cost": TypeAdapter(programs.ActivateCostHead, config=CONFIG),
    "A.head.activate.fusion": TypeAdapter(programs.ActivateFusionHead, config=CONFIG),
    "A.head.activate": TypeAdapter(programs.ActivateHead, config=CONFIG),
    "A.head.evolve": TypeAdapter(programs.EvolveHead, config=CONFIG),
    "A.head.ride": TypeAdapter(programs.RideHead, config=CONFIG),
    "A.head.feed": TypeAdapter(programs.FeedHead, config=CONFIG),
    "A.head.spell": TypeAdapter(programs.SpellHead, config=CONFIG),
    "A.head.static": TypeAdapter(programs.StaticHead, config=CONFIG),
    "A.play.zones": TypeAdapter(programs.PlayZones, config=CONFIG),
    "A.play.conditional-zones": TypeAdapter(
        programs.ConditionalPlayZones, config=CONFIG
    ),
    "A.play.playable-from": TypeAdapter(programs.PlayableFrom, config=CONFIG),
    "A.head.play": TypeAdapter(programs.PlayHead, config=CONFIG),
    "A.granted-ability-head": TypeAdapter(programs.GrantedAbilityHead),
    "A.ability-head": TypeAdapter(programs.AbilityHead, config=CONFIG),
    "A.effect.grant": TypeAdapter(programs.GrantEffect, config=CONFIG),
    "A.act.delayed": TypeAdapter(programs.DelayedAction, config=CONFIG),
}

ABILITY_NAMES = {
    "A.ability-zone": "AbilityZone",
    "A.ability-zones": "AbilityZones",
    "A.define": "Definitions",
    "A.evolution-destination": "EvolutionDestination",
    "A.fusion.exact": "FusionExact",
    "A.fusion.up-to": "FusionUpTo",
    "A.fusion.any": "FusionAny",
    "A.fusion": "Fusion",
    "A.head.trigger": "TriggerHead",
    "A.head.lastword": "LastwordHead",
    "A.head.auto": "AutoHead",
    "A.head.activate.cost": "ActivateCostHead",
    "A.head.activate.fusion": "ActivateFusionHead",
    "A.head.activate": "ActivateHead",
    "A.head.evolve": "EvolveHead",
    "A.head.ride": "RideHead",
    "A.head.feed": "FeedHead",
    "A.head.spell": "SpellHead",
    "A.head.static": "StaticHead",
    "A.play.zones": "PlayZones",
    "A.play.conditional-zones": "ConditionalPlayZones",
    "A.play.playable-from": "PlayableFrom",
    "A.head.play": "PlayHead",
    "A.granted-ability-head": "GrantedAbilityHead",
    "A.ability-head": "AbilityHead",
    "A.effect.grant": "GrantEffect",
    "A.act.delayed": "DelayedAction",
}
