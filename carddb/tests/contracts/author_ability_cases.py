"""Card-derived ability fragments; source locations and partial status are documented."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pydantic import JsonValue

CARD_ABILITIES: list[tuple[str, str, dict[str, JsonValue]]] = [
    ("BP01-159", "keywords", {"keywords": ["ward"]}),
    ("BP01-159", "lastword", {"lastword": {"do": [{"draw": {"n": 1}}]}}),
    ("BP01-007", "quick", {"quick": True}),
    (
        "BP01-011",
        "fanfare",
        {
            "fanfare": {
                "do": [
                    {
                        "buff": {
                            "on": {
                                "any_of": [
                                    {
                                        "who": "self",
                                        "zone": "field",
                                        "trait": "妖精",
                                        "kind": "follower",
                                        "other": True,
                                    },
                                    {
                                        "who": "self",
                                        "zone": "ex",
                                        "trait": "妖精",
                                        "kind": "follower",
                                    },
                                ]
                            },
                            "power": 1,
                            "hp": 1,
                        }
                    }
                ]
            }
        },
    ),
    (
        "BP02-047",
        "evolve_time",
        {"evolve_time": {"do": [{"draw": {"n": 1}}, {"discard": {"n": 1}}]}},
    ),
    (
        "BP02-019",
        "attack_time",
        {"attack_time": {"per_turn": 1, "do": [{"stand": "this"}]}},
    ),
    ("CP03-008", "ride", {"ride": {"cost": {"pp": 1}}}),
    ("CP01-042", "feed-None", {"feed": {"cost": {"pp": 1}}}),
    ("CP01-042", "feed-2", {"feed": {"cost": {"pp": 2}, "times": 2}}),
    ("CP01-042", "feed-3", {"feed": {"cost": {"pp": 3}, "times": 3}}),
    (
        "BP09-004",
        "evolve-list",
        {
            "evolve": {
                "cost": {"pp": 1},
                "into": ["深緑の純心・ポーラ", "真紅の絆・ポーラ"],
            }
        },
    ),
    (
        "BP03-056",
        "evolve-selector",
        {
            "evolve": {
                "cost": {"pp": 1},
                "into": {
                    "who": "self",
                    "zone": "evolve_deck",
                    "kind": "evolve_follower",
                    "name_contains": "レーヴァテインドラゴン",
                },
            }
        },
    ),
    (
        "BP02-038",
        "conditional-cost",
        {
            "play": {
                "cost_delta": {
                    "if": {
                        "exists": {
                            "who": "self",
                            "zone": "field",
                            "name": "マナリアドラコ・グレア",
                        }
                    },
                    "delta": -2,
                }
            }
        },
    ),
    (
        "BP03-097",
        "conditional-cost",
        {
            "play": {
                "cost_delta": {
                    "if": {"leader_life": {"who": "self", "le": 5}},
                    "delta": -5,
                }
            }
        },
    ),
    (
        "BP04-023",
        "conditional-cost",
        {
            "play": {
                "cost_delta": {
                    "if": {
                        "cmp": {
                            "left": {"count": {"who": "opp", "zone": "field"}},
                            "op": "ge",
                            "right": 3,
                        }
                    },
                    "delta": -2,
                }
            }
        },
    ),
    (
        "BP13-093",
        "cost-list",
        {
            "play": {
                "cost_delta": [
                    {
                        "if": {
                            "cmp": {
                                "left": {"count": {"who": "self", "zone": "field"}},
                                "op": "eq",
                                "right": 0,
                            }
                        },
                        "delta": -5,
                    },
                    {
                        "if": {
                            "cmp": {
                                "left": {
                                    "count": {
                                        "who": "self",
                                        "zone": "hand",
                                        "other": True,
                                    }
                                },
                                "op": "le",
                                "right": 2,
                            }
                        },
                        "delta": -5,
                    },
                    {
                        "if": {
                            "cmp": {
                                "left": {"count": {"who": "self", "zone": "cemetery"}},
                                "op": "ge",
                                "right": 15,
                            }
                        },
                        "delta": -5,
                    },
                ]
            }
        },
    ),
    (
        "BP04-103",
        "leader-grant",
        {
            "fanfare": {
                "do": [
                    {
                        "grant": {
                            "on": {
                                "any_of": [
                                    {"leader": "self"},
                                    {
                                        "who": "self",
                                        "zone": "field",
                                        "kind": "follower",
                                    },
                                ]
                            },
                            "abilities": [
                                {
                                    "static": {
                                        "effects": [
                                            {
                                                "prevent": {
                                                    "on": "this",
                                                    "kind": "ability",
                                                }
                                            }
                                        ]
                                    }
                                }
                            ],
                            "until": "turn",
                        }
                    }
                ]
            }
        },
    ),
    (
        "BP12-029",
        "mixed-static",
        {
            "static": {
                "effects": [
                    {
                        "grant": {
                            "on": {
                                "who": "self",
                                "zone": "field",
                                "kind": "follower",
                                "name": "ミストデューク・アゾード",
                            },
                            "keywords": ["storm"],
                            "abilities": [
                                {
                                    "attack_time": {
                                        "do": [
                                            {
                                                "buff": {
                                                    "on": "this",
                                                    "power": 2,
                                                    "hp": 2,
                                                }
                                            }
                                        ]
                                    }
                                }
                            ],
                        }
                    }
                ]
            }
        },
    ),
    (
        "BP13-033",
        "mixed-static",
        {
            "static": {
                "effects": [
                    {
                        "grant": {
                            "on": "this",
                            "keywords": ["assail"],
                            "abilities": [
                                {
                                    "attack_time": {
                                        "do": [
                                            {
                                                "buff": {
                                                    "on": "this",
                                                    "power": 2,
                                                    "hp": 1,
                                                }
                                            }
                                        ]
                                    }
                                }
                            ],
                        }
                    }
                ],
                "while": {
                    "cmp": {
                        "left": {
                            "count": {
                                "who": "self",
                                "zone": "cemetery",
                                "trait": "レヴィオン",
                            }
                        },
                        "op": "ge",
                        "right": 5,
                    }
                },
            }
        },
    ),
    (
        "CP03-023",
        "mixed-static",
        {
            "static": {
                "effects": [
                    {
                        "grant": {
                            "on": "this",
                            "keywords": ["storm"],
                            "abilities": [
                                {
                                    "attack_time": {
                                        "do": [
                                            {
                                                "buff": {
                                                    "on": "this",
                                                    "power": 1,
                                                    "hp": 1,
                                                }
                                            }
                                        ]
                                    }
                                }
                            ],
                        }
                    }
                ],
                "while": {
                    "cmp": {
                        "left": {
                            "count": {
                                "who": "self",
                                "zone": "cemetery",
                                "trait": "ロイヤルパラディン",
                            }
                        },
                        "op": "ge",
                        "right": 15,
                    }
                },
            }
        },
    ),
    (
        "ECP01-043",
        "mixed-static",
        {
            "static": {
                "effects": [
                    {
                        "grant": {
                            "on": "this",
                            "keywords": ["storm"],
                            "abilities": [
                                {
                                    "attack_time": {
                                        "do": [
                                            {
                                                "buff": {
                                                    "on": "this",
                                                    "power": 3,
                                                    "hp": 3,
                                                }
                                            }
                                        ]
                                    }
                                }
                            ],
                        }
                    }
                ],
                "while": {
                    "cmp": {
                        "left": {
                            "count": {
                                "who": "self",
                                "zone": "cemetery",
                                "trait": "ウマ娘",
                            }
                        },
                        "op": "ge",
                        "right": 20,
                    }
                },
            }
        },
    ),
    (
        "BP18-081",
        "nested-static",
        {
            "super_evolve_time": {
                "do": [
                    {
                        "grant": {
                            "on": "this",
                            "abilities": [
                                {
                                    "static": {
                                        "effects": [
                                            {
                                                "keywords": {
                                                    "on": {
                                                        "who": "self",
                                                        "zone": "field",
                                                        "kind": "follower",
                                                        "name": "フォレストバット",
                                                    },
                                                    "list": ["storm", "bane"],
                                                }
                                            }
                                        ]
                                    }
                                }
                            ],
                        }
                    }
                ]
            }
        },
    ),
    (
        "CP04-PR21",
        "equipment-nested-static",
        {
            "static": {
                "zone": "equipment",
                "effects": [
                    {
                        "grant": {
                            "on": "host",
                            "abilities": [
                                {
                                    "static": {
                                        "effects": [
                                            {
                                                "damage_mod": {
                                                    "source": {
                                                        "any_of": [
                                                            {"in": "this"},
                                                            {
                                                                "who": "self",
                                                                "zone": "field",
                                                                "name": "ミラーイメージ・ネネカ",
                                                            },
                                                        ]
                                                    },
                                                    "kind": "ability",
                                                    "delta": 1,
                                                }
                                            }
                                        ]
                                    }
                                }
                            ],
                        }
                    }
                ],
            }
        },
    ),
    (
        "BP01-056",
        "delayed-once-partial",
        {
            "spell": {
                "do": [
                    {
                        "delayed": {
                            "on": {"phase": {"which": "end", "whose": "self"}},
                            "do": [{"banish": {"ref": {"who": "self", "zone": "ex"}}}],
                        }
                    }
                ]
            }
        },
    ),
    (
        "BP03-089",
        "delayed-next-partial",
        {
            "spell": {
                "do": [
                    {
                        "delayed": {
                            "on": {
                                "enter_field": {
                                    "who": "self",
                                    "sel": {"kind": "follower"},
                                    "entered_by": "play",
                                }
                            },
                            "until": "turn",
                            "next_only": True,
                            "do": [{"buff": {"on": "it", "power": 1, "hp": 1}}],
                        }
                    }
                ]
            }
        },
    ),
    (
        "CP01-007",
        "race-time-partial",
        {"race_time": {"do": [{"buff": {"on": "this", "power": 1, "hp": 1}}]}},
    ),
    (
        "CP03-008",
        "drive-time-partial",
        {"drive_time": {"do": [{"buff": {"on": "this", "power": 1, "hp": 1}}]}},
    ),
    (
        "BP08-020",
        "attack-limit-two",
        {"attack_time": {"per_turn": 2, "do": [{"stand": "this"}]}},
    ),
    (
        "BP08-084",
        "activate-limit-two",
        {
            "activate": {
                "per_turn": 2,
                "cost": {"leader_life": 1},
                "targets": [
                    {
                        "bind": "t",
                        "from": {"who": "opp", "zone": "field", "kind": "follower"},
                        "n": 1,
                    }
                ],
                "do": [{"damage": {"to": "t", "n": 1}}],
            }
        },
    ),
    (
        "BP07-036",
        "auto-limit-four",
        {
            "auto": {
                "on": {"played": {"who": "self", "sel": {"trait": "機械"}}},
                "per_turn": 4,
                "targets": [
                    {
                        "bind": "t",
                        "from": {"who": "opp", "zone": "field", "kind": "follower"},
                        "n": 1,
                    }
                ],
                "do": [{"damage": {"to": "t", "n": 1}}],
            }
        },
    ),
    (
        "BP20-020",
        "fusion-up-to",
        {
            "activate": {
                "zone": "hand",
                "fusion": {
                    "sel": {
                        "who": "self",
                        "trait": "財宝",
                        "orig_cost_ge": 1,
                    },
                    "upto": 2,
                    "result": "fused",
                },
                "x": {"from_payment": "fused"},
                "do": [{"counter": {"on": "this", "name": "融合", "delta": "X"}}],
            }
        },
    ),
    (
        "BP15-046",
        "spell-optional-x-partial",
        {
            "spell": {
                "optional_cost": {"pp": "X"},
                "x": {},
                "do": [
                    {
                        "search": {
                            "sel": {
                                "who": "self",
                                "zone": "deck",
                                "kind": "follower",
                                "trait": "陰陽師",
                                "orig_cost_le": "X",
                            },
                            "n": 1,
                            "to": "field",
                        }
                    }
                ],
            }
        },
    ),
    (
        "CP04-PR20",
        "equipment-auto-limit",
        {
            "static": {
                "zone": "equipment",
                "effects": [
                    {
                        "grant": {
                            "on": "host",
                            "abilities": [
                                {
                                    "auto": {
                                        "on": {
                                            "played": {
                                                "who": "self",
                                                "sel": {"kind": "spell"},
                                            }
                                        },
                                        "per_turn": 2,
                                        "do": [
                                            {
                                                "damage": {
                                                    "to": {"leaders": "opponents"},
                                                    "n": {
                                                        "half_up": {"power_of": "this"}
                                                    },
                                                }
                                            }
                                        ],
                                    }
                                }
                            ],
                        }
                    }
                ],
            }
        },
    ),
    (
        "CP04-PR18",
        "equipment-ub-activate",
        {
            "static": {
                "zone": "equipment",
                "effects": [
                    {
                        "grant": {
                            "on": "host",
                            "abilities": [
                                {
                                    "activate": {
                                        "ub": True,
                                        "cost": {
                                            "steps": [{"pp": 1}, {"act_self": True}]
                                        },
                                        "do": [
                                            {
                                                "buff": {
                                                    "on": {
                                                        "who": "self",
                                                        "zone": "field",
                                                        "kind": "follower",
                                                        "orig_cost_eq": 1,
                                                    },
                                                    "power": 1,
                                                }
                                            }
                                        ],
                                    }
                                }
                            ],
                        }
                    }
                ],
            }
        },
    ),
    (
        "CP04-012",
        "grant-full-next-turn",
        {
            "activate": {
                "ub": True,
                "cost": {"act_self": True},
                "do": [
                    {
                        "grant": {
                            "on": "this",
                            "until": {"during_next_turn": {"who": "opp"}},
                            "abilities": [
                                {
                                    "static": {
                                        "while": {"acted": "this"},
                                        "effects": [
                                            {
                                                "must_attack": {
                                                    "on": {
                                                        "who": "opp",
                                                        "zone": "field",
                                                        "kind": "follower",
                                                    }
                                                }
                                            }
                                        ],
                                    }
                                }
                            ],
                        }
                    }
                ],
            }
        },
    ),
    (
        "CP04-045",
        "ex-granted-auto-fragment",
        {
            "fanfare": {
                "do": [
                    {"move": {"ref": "t", "to": "ex", "result": "moved"}},
                    {
                        "grant": {
                            "on": {"follow": "moved"},
                            "abilities": [
                                {
                                    "auto": {
                                        "zone": "ex",
                                        "on": {
                                            "phase": {"which": "end", "whose": "self"}
                                        },
                                        "do": [
                                            {"move": {"ref": "this", "to": "cemetery"}}
                                        ],
                                    }
                                }
                            ],
                        }
                    },
                ]
            }
        },
    ),
    (
        "SP01-026",
        "delayed-batch-mode-fragment",
        {
            "activate": {
                "cost": {"act_self": True},
                "do": [
                    {
                        "delayed": {
                            "on": {
                                "enter_field": {
                                    "who": "self",
                                    "sel": {"kind": "follower"},
                                }
                            },
                            "aggregation": "batch",
                            "next_only": True,
                            "until": "turn",
                            "do": [
                                {
                                    "choose": {
                                        "bind": "selected",
                                        "from": {"in": "it"},
                                        "n": 1,
                                    }
                                },
                                {"buff": {"on": "selected", "power": 1, "hp": 1}},
                            ],
                        }
                    }
                ],
            }
        },
    ),
]

CARD_ABILITIES.extend(
    [
        (
            "BP18-007",
            "conditional-zone",
            {
                "play": {
                    "playable_from": {
                        "zone": "cemetery",
                        "if": {
                            "history": {
                                "event": "played",
                                "window": "this_turn",
                                "who": "self",
                                "at_least": 3,
                            }
                        },
                    }
                }
            },
        ),
        (
            "BP20-030",
            "fusion-exact-fragment",
            {
                "activate": {
                    "zone": "hand",
                    "fusion": {
                        "sel": {
                            "who": "self",
                            "trait": "財宝",
                            "orig_cost_ge": 1,
                        },
                        "n": 3,
                    },
                    "do": [{"counter": {"on": "this", "name": "融合", "delta": 1}}],
                }
            },
        ),
        (
            "CP04-PR25",
            "equipment-batch",
            {
                "static": {
                    "zone": "equipment",
                    "effects": [
                        {
                            "grant": {
                                "on": "host",
                                "abilities": [
                                    {
                                        "auto": {
                                            "on": {
                                                "dealt_damage": {
                                                    "ref": "this",
                                                    "kind": "ability",
                                                    "recipient": {
                                                        "who": "opp",
                                                        "zone": "field",
                                                        "kind": "follower",
                                                    },
                                                }
                                            },
                                            "aggregation": "batch",
                                            "do": [
                                                {
                                                    "damage": {
                                                        "to": {"leaders": "opponents"},
                                                        "n": 3,
                                                    }
                                                }
                                            ],
                                        }
                                    }
                                ],
                            }
                        }
                    ],
                }
            },
        ),
        (
            "CP04-058",
            "equipment-mixed-section",
            {
                "static": {
                    "zone": "equipment",
                    "effects": [
                        {
                            "grant": {
                                "on": "host",
                                "keywords": ["storm"],
                                "abilities": [
                                    {
                                        "attack_time": {
                                            "targets": [
                                                {
                                                    "bind": "t",
                                                    "from": {
                                                        "who": "opp",
                                                        "zone": "field",
                                                        "kind": "follower",
                                                    },
                                                    "upto": 2,
                                                }
                                            ],
                                            "do": [{"damage": {"to": "t", "n": 3}}],
                                        }
                                    }
                                ],
                            }
                        }
                    ],
                }
            },
        ),
        (
            "BP14-018",
            "advance-activate",
            {
                "activate": {
                    "advance": True,
                    "cost": {"steps": [{"pp": 4}, {"banish_self": True}]},
                    "usable_if": {
                        "any": [
                            {
                                "cmp": {
                                    "left": {
                                        "count": {
                                            "who": "self",
                                            "zone": "cemetery",
                                            "trait": "宴楽",
                                        }
                                    },
                                    "op": "ge",
                                    "right": 5,
                                }
                            },
                            {
                                "cmp": {
                                    "left": {
                                        "count": {
                                            "who": "self",
                                            "zone": "cemetery",
                                            "class": "ロイヤル",
                                        }
                                    },
                                    "op": "ge",
                                    "right": 10,
                                }
                            },
                        ]
                    },
                    "do": [
                        {
                            "choose": {
                                "bind": "chosen",
                                "from": {
                                    "who": "self",
                                    "zone": "evolve_deck",
                                    "name": "楽園の開闢・タケツミ",
                                },
                                "n": 1,
                                "may": True,
                            }
                        },
                        {"move": {"ref": "chosen", "to": "field"}},
                    ],
                }
            },
        ),
    ]
)
