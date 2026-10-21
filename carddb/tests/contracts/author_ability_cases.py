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
]
