"""Card-grounded programs and explicitly synthetic action/control boundary cases."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pydantic import JsonValue

CARD_STEPS: list[tuple[str, str, list[JsonValue]]] = [
    (
        "CP03-105",
        "text/fanfare",
        [
            {"look": {"top": {"who": "self", "n": 3}, "bind": "seen"}},
            {
                "choose": {
                    "bind": "picked",
                    "from": {"in": "seen"},
                    "any": True,
                    "order": True,
                }
            },
            {
                "move": {
                    "ref": "picked",
                    "to": "deck",
                    "position": "top",
                    "result": "placed",
                }
            },
            {
                "move": {
                    "ref": {
                        "in": "seen",
                        "except": {"in": {"objects_of": "placed", "view": "before"}},
                    },
                    "to": "deck",
                    "position": "bottom",
                    "order": "chooser",
                }
            },
        ],
    ),
]

# Synthetic examples exercise documented variants without inventing card evidence.
SYNTHETIC_ACTIONS: list[dict[str, JsonValue]] = [
    {"choose": {"bind": "picked", "from": {"leader": "opp"}, "n": 1}},
    {
        "choose": {
            "bind": "picked",
            "from": {"who": "opp", "kind": "follower", "leader_or": True},
            "upto": 1,
        }
    },
    {
        "choose": {
            "bind": "picked",
            "from": {"who": "self", "zone": "hand"},
            "upto": 2,
            "groups": [{"kind": "follower"}, {"kind": "spell"}],
        }
    },
    {
        "choose": {
            "bind": "picked",
            "from": {"in": "seen"},
            "n": 2,
            "groups": [{"orig_cost_eq": 1}, {"orig_cost_eq": 2}],
        }
    },
    {
        "choose": {
            "bind": "picked",
            "from": {"in": "seen"},
            "any": True,
            "groups": [{"kind": "follower"}, {"kind": "amulet"}],
        }
    },
    {
        "search": {
            "groups": [
                {"sel": {"who": "self", "zone": "deck", "kind": "follower"}, "upto": 1},
                {"sel": {"who": "self", "zone": "deck", "kind": "spell"}, "n": 1},
            ],
            "to": "hand",
            "entry_effects": [{"no_trigger": {"on": "entering", "kinds": ["fanfare"]}}],
        }
    },
    {
        "search": {
            "sel": {"who": "self", "zone": "deck"},
            "n": 1,
            "to": "hand",
            "instead": [
                {
                    "if": {"paid": "optional"},
                    "replace_with": {
                        "search": {
                            "sel": {"who": "self", "zone": "deck"},
                            "upto": 2,
                            "to": "field",
                        }
                    },
                }
            ],
        }
    },
    {"draw": {"n": {"param": "count"}, "who": {"param": "player"}}},
    {
        "draw": {
            "n": 1,
            "who": "each",
            "instead": [
                {
                    "if": {"awakened": True},
                    "n": 2,
                    "after": [{"life": {"who": "self", "delta": 1}}],
                }
            ],
        }
    },
    {"discard": {"n": 1, "who": "self", "random": True}},
    {"discard": {"all": True}},
    {"discard": {"down_to": 3}},
    {"discard": {"any": True}},
    {"damage": {"to": {"leader": "opp"}, "n": {"param": "damage"}}},
    {"damage": {"to": "t", "n": 3, "distribute": True, "distribute_at": "resolution"}},
    {"destroy": "t"},
    {"destroy": {"ref": {"who": "opp", "zone": "field"}}},
    {"banish": "t"},
    {"banish": {"ref": "t", "result": "gone"}},
    {"banish": {"top": {"who": "self", "n": 1}}},
    {"move": {"top": {"who": "self", "n": 2}, "to": "cemetery"}},
    {
        "move": {
            "ref": "t",
            "to": "deck",
            "position": {"one_of": [{"nth": 2}, "bottom"], "by": "self"},
            "order": "shuffle",
        }
    },
    {"move": {"ref": "t", "to": {"param": "destination"}, "position": {"nth": 1}}},
    {
        "move": {
            "ref": "t",
            "to": "field",
            "acted": False,
            "entry_effects": [{"stat": {"power": 1}}],
        }
    },
    {
        "look": {
            "ref": {"who": "opp", "zone": "hand"},
            "bind": "seen",
            "by": "self",
            "random": {"n": 1},
        }
    },
    {"reveal": {"top": {"who": "self", "n": 1}, "bind": "shown"}},
    {
        "reveal": {
            "ref": {"who": "self", "zone": "hand"},
            "bind": "shown",
            "random": {"n": 1},
        }
    },
    {
        "reveal_until": {
            "sel": {"who": "self", "zone": "deck", "kind": "follower"},
            "n": 1,
            "bind": "seen",
            "matched": "hits",
        }
    },
    {"act": {"ref": "t"}},
    {"stand": "this"},
    {"stand": {"ref": "t"}},
    {"flip_down": "t"},
    {"flip_down": {"ref": "t"}},
    {"flip_up": "t"},
    {"flip_up": {"ref": "t"}},
    {"life": {"who": "self", "delta": {"param": "recovery"}}},
    {"buff": {"on": "this", "power": 1}},
    {"buff": {"on": "t", "hp": -1}},
    {"set_stat": {"on": "t", "hp": 1}},
    {"grant": {"on": "t", "keywords": ["疾走", {"param": "keyword"}]}},
    {"lose_abilities": {"on": "t"}},
    {"counter": {"on": "this", "name": "stack", "delta": -1}},
    {"pp": {"who": "self", "delta": 1}},
    {"pp": {"who": "self", "to_max": True}},
    {"pp_max": {"who": "self", "delta": 1}},
    {"ep": {"who": "self", "delta": 1}},
    {"sep": {"who": "self", "delta": 1}},
    {"shuffle": {"who": "self"}},
    {"dice": {"bind": "rolled"}},
    {"declare": {"bind": "declared", "kind": "number", "min": 1, "max": 9}},
    {"declare": {"bind": "named", "kind": "name"}},
    {"steal": {"ref": "t", "to": "self"}},
    {"give": {"ref": "this", "to": "other_players"}},
    {"stack_plus": 1},
    {"race": {"ref": "this", "n": 2}},
    {"gain_drive": {"ref": "this"}},
    {"play_card": {"ref": "t"}},
    {
        "play_ability": {
            "of": "t",
            "trigger": "fanfare",
            "cost_policy": "waive_original",
        }
    },
    {"play_ability": {"of": "this", "ability": {"ub": True, "n": 10}}},
    {"apply": {"on": "t", "effect": {"stat": {"power": 1}}, "capture": "play"}},
    {
        "player_effect": {
            "who": "self",
            "effect": {
                "cost_mod": {
                    "when_playing": {"who": "self", "zone": "hand"},
                    "delta": -1,
                }
            },
        }
    },
    {"win": {"who": "self"}},
    {"skip_turn": {"who": "self"}},
    {"extra_turn": {"who": "self"}},
    {"make_type": {"on": "this", "type": "amulet"}},
    {"create": {"token": "token", "to": "field"}},
    {
        "create": {
            "token": ["first", {"param": "token"}],
            "to": "ex",
            "n": {"param": "count"},
        }
    },
    {"create": {"token": "token", "to": "field", "upto": 2, "acted": True}},
    {"create": {"token": "token", "to": "field", "any": True}},
    {
        "create": {
            "token": {"allocate": {"among": ["first", "second"], "total": 3}},
            "to": {"one_of": ["field", "ex"], "by": "self", "scope": "each"},
            "bind": "created",
            "counters": {"stack": 1},
        }
    },
    {"evolve": {"ref": "this"}},
    {"evolve": {"ref": "t", "into": ["first", "second"]}},
    {"evolve": {"ref": "t", "into": {"who": "self", "zone": "deck"}}},
    {"transform": {"ref": "t", "into": "token"}},
    {
        "transform": {
            "ref": "t",
            "into": {
                "allocate": {"among": ["first", "second"], "each_exactly_once": True}
            },
        }
    },
]

SYNTHETIC_CONTROLS: list[dict[str, JsonValue]] = [
    {"optional": {"do": [{"draw": {"n": 1}}], "then": [], "result": "attempted"}},
    {"pay": {"cost": {"pp": 1}, "do": [], "then": []}},
    {
        "choice": {
            "upto": 1,
            "options": [[], [{"draw": {"n": 1}}]],
            "exclude_chosen": "this_turn",
        }
    },
    {"either": {"options": [[], []], "bind": "selected"}},
    {"if": {"cond": {"awakened": True}, "then": [], "else": []}},
    {"repeat": {"n": 2, "do": [{"draw": {"n": 1}}], "collect": {"receipts": "rounds"}}},
    {
        "repeat": {
            "until": {"hand": {"who": "self", "eq": 5}},
            "do": [],
            "check": "before",
        }
    },
    {"repeat": {"while": {"awakened": True}, "do": [], "check": "after"}},
    {"repeat_ability": {"while": {"awakened": True}}},
    {"foreach": {"in": "t", "as": "subject", "do": [{"act": "subject"}]}},
    {
        "each_player": {
            "stages": [
                [{"draw": {"n": 1, "who": "item"}}],
                {"do": [], "mode": "choice"},
                {"do": [], "mode": "nonchoice"},
                {"do": [], "mode": "explicit", "evidence": "synthetic"},
            ]
        }
    },
    {
        "each_player": {
            "as": "participant",
            "who": "each",
            "stages": [[{"draw": {"who": "participant", "n": 1}}]],
        }
    },
    {"simultaneous": {"do": [{"act": "t"}], "result": "batch"}},
    {"by_kind": {"ref": "t", "leader": [], "follower": [], "as": "subject"}},
]

CARD_STEPS.extend(
    [
        (
            "ECP02-034",
            "text/fanfare",
            [
                {"look": {"top": {"who": "self", "n": 3}, "bind": "seen"}},
                {
                    "choose": {
                        "bind": "picked",
                        "from": {"in": "seen"},
                        "upto": 3,
                        "groups": [
                            {"trait": "キュート"},
                            {"trait": "クール"},
                            {"trait": "パッション"},
                        ],
                        "reveal": True,
                        "may": True,
                    }
                },
                {"move": {"ref": "picked", "to": "hand", "result": "taken"}},
                {
                    "move": {
                        "ref": {
                            "in": "seen",
                            "except": {"in": {"objects_of": "taken", "view": "before"}},
                        },
                        "to": "deck",
                        "position": "bottom",
                        "order": "chooser",
                    }
                },
            ],
        ),
        (
            "CP04-P68",
            "sections/0",
            [
                {
                    "search": {
                        "groups": [
                            {
                                "sel": {
                                    "who": "self",
                                    "zone": "deck",
                                    "kind": "follower",
                                    "trait": "ルーセント学院",
                                    "orig_cost_le": 3,
                                },
                                "n": 1,
                            },
                            {
                                "sel": {
                                    "who": "self",
                                    "zone": "deck",
                                    "kind": "follower",
                                    "trait": "ルーセント学院",
                                    "orig_cost_eq": 1,
                                },
                                "n": 1,
                            },
                        ],
                        "to": "field",
                        "entry_effects": [
                            {"no_trigger": {"on": "entering", "kinds": ["fanfare"]}}
                        ],
                    }
                }
            ],
        ),
        (
            "BP13-035",
            "text/search",
            [
                {
                    "search": {
                        "sel": {
                            "who": "self",
                            "zone": "deck",
                            "names": [
                                "レヴィオンの副団長・マイム",
                                "レヴィオンの魔術師・ミイム",
                                "レヴィオンの武闘家・メイム",
                            ],
                        },
                        "n": 1,
                        "to": "hand",
                        "instead": [
                            {
                                "if": {"paid": "optional"},
                                "replace_with": {
                                    "search": {
                                        "groups": [
                                            {
                                                "sel": {
                                                    "who": "self",
                                                    "zone": "deck",
                                                    "name": "レヴィオンの副団長・マイム",
                                                },
                                                "n": 1,
                                            },
                                            {
                                                "sel": {
                                                    "who": "self",
                                                    "zone": "deck",
                                                    "name": "レヴィオンの魔術師・ミイム",
                                                },
                                                "n": 1,
                                            },
                                            {
                                                "sel": {
                                                    "who": "self",
                                                    "zone": "deck",
                                                    "name": "レヴィオンの武闘家・メイム",
                                                },
                                                "n": 1,
                                            },
                                        ],
                                        "to": "field",
                                    }
                                },
                            }
                        ],
                    }
                }
            ],
        ),
    ]
)
