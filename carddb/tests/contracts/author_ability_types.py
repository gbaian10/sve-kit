"""Independent Python correspondence for the closed ability graph."""

from sve_carddb.contracts import author_programs as programs

from .author_adapters import adapters

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
    "A.companion-head": "CompanionHead",
    "A.effect.grant": "GrantEffect",
    "A.act.delayed": "DelayedAction",
}


ABILITIES = adapters(
    {key: (getattr(programs, name), name) for key, name in ABILITY_NAMES.items()}
)
