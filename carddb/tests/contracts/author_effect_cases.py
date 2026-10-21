"""Independent effect adapters and author fragments; no execution or scope admission."""

from typing import TYPE_CHECKING

from sve_carddb.contracts import author_programs as effects

from .author_adapters import adapters

if TYPE_CHECKING:
    from pydantic import JsonValue

EFFECT_NAMES = {
    "A.effect-object": "EffectObject",
    "A.effect-recipient": "EffectRecipient",
    "A.effect-players": "EffectPlayers",
    "A.effect.stat": "Stat",
    "A.effect.set-stat": "SetStat",
    "A.effect.keywords": "Keywords",
    "A.effect.lose-abilities": "LoseAbilities",
    "A.effect.also-named": "AlsoNamed",
    "A.effect.trait": "TraitEffect",
    "A.effect.make-type": "MakeType",
    "A.effect.cost-mod": "CostMod",
    "A.effect.damage-mod": "DamageMod",
    "A.effect.prevent": "Prevent",
    "A.effect.damage-cap": "DamageCap",
    "A.effect.evolve-cost-mod": "EvolveCostMod",
    "A.effect.banish-instead": "BanishReplacementEffect",
    "A.effect.object": "ObjectEffect",
    "A.effect.play-permission": "PlayPermission",
    "A.effect.no-trigger": "NoTrigger",
    "A.effect.player": "PlayerEffect",
    "A.effect.reroll": "Reroll",
    "A.effect.extra-trigger": "ExtraTrigger",
    "A.effect.must-target": "MustTarget",
    "A.effect.rule.spellchain-population": "SpellchainPopulation",
    "A.effect.rule.counter-cap": "CounterCap",
    "A.effect.rule.battle-damage-basis": "BattleDamageBasis",
    "A.effect.rule-override": "RuleOverride",
    "A.effect.card-predicate": "CardPredicate",
    "A.effect.ability.keyword": "KeywordAbility",
    "A.effect.ability.activated": "ActivatedAbility",
    "A.effect.ability-predicate": "AbilityPredicate",
    "A.effect.cannot.draw-not-phase": "DrawNotPhase",
    "A.effect.cannot.draw-rule-phase": "DrawRulePhase",
    "A.effect.cannot.stand-rule-phase": "StandRulePhase",
    "A.effect.cannot.play-card": "CannotPlayCard",
    "A.effect.cannot.play-ability": "CannotPlayAbility",
    "A.effect.cannot.attack": "CannotAttack",
    "A.effect.cannot.choose-target": "CannotChooseTarget",
    "A.effect.cannot.draw": "CannotDraw",
    "A.effect.cannot.remove": "CannotRemove",
    "A.effect.cannot.stand": "CannotStand",
    "A.effect.cannot.player": "CannotPlayer",
    "A.effect.cannot.deal-damage": "CannotDealDamage",
    "A.effect.cannot": "Cannot",
    "A.effect": "EffectSpec",
}


CARD_EFFECTS: list[tuple[str, str, dict[str, JsonValue]]] = [
    (
        "BP06-054",
        "static",
        {
            "keywords": {
                "on": {
                    "who": "self",
                    "zone": "field",
                    "kind": "follower",
                    "class": "ドラゴン",
                    "other": True,
                },
                "list": ["ward"],
            }
        },
    ),
    (
        "BP08-004",
        "token-section:0/static",
        {"also_named": {"on": "this", "name": "操り人形"}},
    ),
    ("BP02-T07", "apply", {"add_trait": {"trait": "武装"}}),
    (
        "BP18-001",
        "apply",
        {"cost_mod": {"when_playing": {"in": "affected"}, "delta": -2}},
    ),
    ("BP01-134", "static", {"damage_mod": {"on": "this", "delta": -1}}),
    ("BP02-090", "apply", {"prevent": {"kind": "any"}}),
    ("BP05-101", "player_effect", {"damage_cap": {"threshold": 5, "set": 4}}),
    (
        "SP01-012",
        "static",
        {
            "evolve_cost_mod": {
                "on": "this",
                "delta": {
                    "minus": [
                        0,
                        {
                            "count": {
                                "who": "self",
                                "zone": "field",
                                "kind": "follower",
                                "other": True,
                            }
                        },
                    ]
                },
            }
        },
    ),
    (
        "BP18-061",
        "static",
        {
            "banish_instead": {
                "on": {"who": "opp", "zone": "field", "kind": "follower"},
                "from": "field",
                "to": "cemetery",
            }
        },
    ),
    (
        "BP05-006",
        "player_effect/draw",
        {"cannot": {"what": "draw", "during": {"phase": "start", "cause": "rule"}}},
    ),
    (
        "ECP01-010",
        "static",
        {"no_trigger": {"who": "opponents", "kinds": ["fanfare", "evolve_time"]}},
    ),
    ("BP18-001", "static", {"unlimited_evolve": {"who": "self"}}),
    ("BP11-112", "static/prospective-entry", {"enters_acted": {"on": "this"}}),
    ("ECP02-065", "static", {"reroll": {"who": "self", "times": 1}}),
    ("BP04-006", "static", {"ignore_ward": {"on": "this"}}),
    ("BP20-P65", "static", {"choice_any": {"who": "self"}}),
    ("BP19-092", "static", {"extra_trigger": {"phase": "end", "who": "self", "n": 1}}),
    ("BP03-091", "static", {"must_target": {"on": "this", "by": "opp"}}),
    (
        "BP02-035",
        "static",
        {
            "rule_override": {
                "rule": "spellchain_population",
                "who": "self",
                "of": {
                    "any_of": [
                        {"who": "self", "zone": "cemetery", "kind": "spell"},
                        {
                            "who": "self",
                            "zone": "cemetery",
                            "kind": "follower",
                            "class": "ウィッチ",
                        },
                    ]
                },
            }
        },
    ),
]

SYNTHETIC_EFFECTS: list[dict[str, JsonValue]] = [
    {"stat": {"on": "this", "power": -1}},
    {"set_stat": {"on": "this", "power": 1, "hp": 1}},
    {"lose_abilities": {"on": "this"}},
    {"remove_trait": {"on": "this", "trait": "武装"}},
    {"make_type": {"on": "this", "type": "amulet"}},
    {"must_attack": {"on": {"who": "opp", "zone": "field", "kind": "follower"}}},
    {"play_permission": {"on": "this", "from": ["cemetery"]}},
    {"cannot": {"what": "choose_target", "on": "this", "by": "opp"}},
    {"cannot": {"what": "banish", "on": "this", "by": "ability"}},
    {"cannot": {"what": "play_card", "who": "self", "card": {"zone": "ex"}}},
]

EFFECTS = adapters(
    {key: (getattr(effects, name), name) for key, name in EFFECT_NAMES.items()}
)
