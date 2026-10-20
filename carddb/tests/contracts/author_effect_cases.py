"""Independent effect adapters and author fragments; no execution or scope admission."""

from typing import TYPE_CHECKING

from pydantic import TypeAdapter

from sve_carddb.contracts import author_effects as effects

from .author_selector_cases import CONFIG

if TYPE_CHECKING:
    from pydantic import JsonValue

EFFECTS: dict[str, TypeAdapter[object]] = {
    "A.effect-object": TypeAdapter(effects.EffectObject, config=CONFIG),
    "A.effect-recipient": TypeAdapter(effects.EffectRecipient, config=CONFIG),
    "A.effect-players": TypeAdapter(effects.EffectPlayers, config=CONFIG),
    "A.effect.stat": TypeAdapter(effects.Stat, config=CONFIG),
    "A.effect.set-stat": TypeAdapter(effects.SetStat, config=CONFIG),
    "A.effect.keywords": TypeAdapter(effects.Keywords, config=CONFIG),
    "A.effect.lose-abilities": TypeAdapter(effects.LoseAbilities, config=CONFIG),
    "A.effect.also-named": TypeAdapter(effects.AlsoNamed, config=CONFIG),
    "A.effect.trait": TypeAdapter(effects.TraitEffect, config=CONFIG),
    "A.effect.make-type": TypeAdapter(effects.MakeType, config=CONFIG),
    "A.effect.cost-mod": TypeAdapter(effects.CostMod, config=CONFIG),
    "A.effect.damage-mod": TypeAdapter(effects.DamageMod, config=CONFIG),
    "A.effect.prevent": TypeAdapter(effects.Prevent, config=CONFIG),
    "A.effect.damage-cap": TypeAdapter(effects.DamageCap, config=CONFIG),
    "A.effect.evolve-cost-mod": TypeAdapter(effects.EvolveCostMod, config=CONFIG),
    "A.effect.banish-instead": TypeAdapter(effects.BanishInstead, config=CONFIG),
    "A.effect.object": TypeAdapter(effects.ObjectEffect, config=CONFIG),
    "A.effect.play-permission": TypeAdapter(effects.PlayPermission, config=CONFIG),
    "A.effect.no-trigger": TypeAdapter(effects.NoTrigger, config=CONFIG),
    "A.effect.player": TypeAdapter(effects.PlayerEffect, config=CONFIG),
    "A.effect.reroll": TypeAdapter(effects.Reroll, config=CONFIG),
    "A.effect.extra-trigger": TypeAdapter(effects.ExtraTrigger, config=CONFIG),
    "A.effect.must-target": TypeAdapter(effects.MustTarget, config=CONFIG),
    "A.effect.rule.spellchain-population": TypeAdapter(
        effects.SpellchainPopulation, config=CONFIG
    ),
    "A.effect.rule.counter-cap": TypeAdapter(effects.CounterCap, config=CONFIG),
    "A.effect.rule.battle-damage-basis": TypeAdapter(
        effects.BattleDamageBasis, config=CONFIG
    ),
    "A.effect.rule-override": TypeAdapter(effects.RuleOverride, config=CONFIG),
    "A.effect.card-predicate": TypeAdapter(effects.CardPredicate, config=CONFIG),
    "A.effect.ability.keyword": TypeAdapter(effects.KeywordAbility, config=CONFIG),
    "A.effect.ability.activated": TypeAdapter(effects.ActivatedAbility, config=CONFIG),
    "A.effect.ability-predicate": TypeAdapter(effects.AbilityPredicate, config=CONFIG),
    "A.effect.cannot.draw-not-phase": TypeAdapter(effects.DrawNotPhase, config=CONFIG),
    "A.effect.cannot.draw-rule-phase": TypeAdapter(
        effects.DrawRulePhase, config=CONFIG
    ),
    "A.effect.cannot.stand-rule-phase": TypeAdapter(
        effects.StandRulePhase, config=CONFIG
    ),
    "A.effect.cannot.play-card": TypeAdapter(effects.CannotPlayCard, config=CONFIG),
    "A.effect.cannot.play-ability": TypeAdapter(
        effects.CannotPlayAbility, config=CONFIG
    ),
    "A.effect.cannot.attack": TypeAdapter(effects.CannotAttack, config=CONFIG),
    "A.effect.cannot.choose-target": TypeAdapter(
        effects.CannotChooseTarget, config=CONFIG
    ),
    "A.effect.cannot.draw": TypeAdapter(effects.CannotDraw, config=CONFIG),
    "A.effect.cannot.remove": TypeAdapter(effects.CannotRemove, config=CONFIG),
    "A.effect.cannot.stand": TypeAdapter(effects.CannotStand, config=CONFIG),
    "A.effect.cannot.player": TypeAdapter(effects.CannotPlayer, config=CONFIG),
    "A.effect.cannot.deal-damage": TypeAdapter(effects.CannotDealDamage, config=CONFIG),
    "A.effect.cannot": TypeAdapter(effects.Cannot, config=CONFIG),
    "A.effect": TypeAdapter(effects.EffectSpec, config=CONFIG),
}

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
    "A.effect.banish-instead": "BanishInstead",
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
    (
        "BP14-046",
        "player_effect",
        {
            "cost_mod": {
                "when_playing": {"in": {"follow": "placed"}},
                "set": 0,
                "next_only": True,
            }
        },
    ),
    ("BP01-134", "static", {"damage_mod": {"on": "this", "delta": -1}}),
    (
        "BP01-137",
        "static/turn-condition-omitted",
        {"damage_mod": {"source": "this", "delta": 4}},
    ),
    (
        "BP02-067",
        "static/leader-attack",
        {
            "damage_mod": {
                "source": "this",
                "on": {"leaders": "all"},
                "kind": "attack",
                "multiply": 2,
            }
        },
    ),
    (
        "BP02-067",
        "static/combat",
        {"damage_mod": {"source": "this", "kind": "combat", "multiply": 2}},
    ),
    ("BP02-090", "apply", {"prevent": {"kind": "any"}}),
    ("BP02-019", "apply", {"prevent": {"kind": "combat"}}),
    (
        "BP21-001",
        "static",
        {
            "prevent": {
                "on": {
                    "any_of": [
                        {
                            "who": "self",
                            "zone": "field",
                            "kind": "follower",
                            "trait": "学院",
                            "not_same_name_as_this": True,
                        },
                        {
                            "who": "self",
                            "zone": "field",
                            "kind": "follower",
                            "trait": "獣",
                        },
                    ]
                },
                "kind": "ability",
            }
        },
    ),
    ("BP05-017", "player_effect", {"prevent": {"kind": "any", "next_only": True}}),
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
    ("BP05-048", "apply", {"evolve_cost_mod": {"on": "affected", "set": 0}}),
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
        "BP10-076",
        "static",
        {
            "cannot": {
                "what": "draw",
                "who": "opponents",
                "during": {"not_phase": "start"},
            }
        },
    ),
    (
        "BP05-006",
        "player_effect/play",
        {"cannot": {"what": "play_card", "card": {"kind": "follower"}}},
    ),
    ("BP05-006", "player_effect/pp", {"cannot": {"what": "pp_max_increase"}}),
    (
        "CP02-005",
        "apply",
        {"cannot": {"what": "play_ability", "ability": {"kind": "activated"}}},
    ),
    (
        "BP03-039",
        "apply",
        {
            "cannot": {
                "what": "play_ability",
                "ability": {"kind": "activated", "except": ["evolution"]},
            }
        },
    ),
    (
        "BP04-038",
        "entry_effects",
        {"cannot": {"what": "play_ability", "ability": {"keyword": "fanfare"}}},
    ),
    (
        "BP05-061",
        "apply",
        {
            "cannot": {
                "what": "attack",
                "target": {"who": "opp", "kind": "follower", "leader_or": True},
            }
        },
    ),
    (
        "BP02-090",
        "static",
        {"cannot": {"what": "destroy", "on": "this", "by": "ability"}},
    ),
    (
        "ECP01-053",
        "static",
        {
            "cannot": {
                "what": "stand",
                "on": "this",
                "during": {"phase": "start", "who": "self", "cause": "rule"},
            }
        },
    ),
    ("BP05-092", "static/win", {"cannot": {"what": "win", "who": "opp"}}),
    ("BP05-092", "static/lose", {"cannot": {"what": "lose", "who": "self"}}),
    ("BP12-109", "apply", {"cannot": {"what": "deal_damage"}}),
    (
        "ECP01-010",
        "static",
        {"no_trigger": {"who": "opponents", "kinds": ["fanfare", "evolve_time"]}},
    ),
    (
        "BP19-074",
        "entry_effects",
        {"no_trigger": {"on": "entering", "kinds": ["fanfare"]}},
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
    (
        "CP04-115",
        "section:0/static",
        {
            "rule_override": {
                "rule": "counter_cap",
                "on": "this",
                "name": "バッテリー",
                "max": 7,
            }
        },
    ),
    (
        "BP01-129",
        "static",
        {
            "rule_override": {
                "rule": "battle_damage_basis",
                "on": {"who": "self", "zone": "field", "kind": "follower"},
                "basis": "hp",
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
