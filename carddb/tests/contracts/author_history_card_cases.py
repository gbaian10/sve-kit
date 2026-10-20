"""Card-labelled fragments from frozen JP text; these do not execute full effects.

Ability-level limits are outside these fragments; a per-turn limit alone does
not establish an event's during filter.
"""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pydantic import JsonValue

CARD_HISTORY_CASES: list[tuple[str, str, JsonValue]] = [
    (
        "BP01-033",
        "A.event",
        {
            "enter_field": {
                "who": "self",
                "sel": {"class": "ロイヤル", "kind": "follower"},
            }
        },
    ),
    (
        "BP10-004",
        "A.event",
        {"enter_field": {"who": "opp", "sel": {"kind": "follower"}}},
    ),
    (
        "BP17-089",
        "A.event",
        {
            "enter_field": {
                "who": "self",
                "sel": {"kind": "follower", "not_class": "ナイトメア"},
            }
        },
    ),
    (
        "BP13-029",
        "A.event",
        {
            "enter_field": {
                "who": "self",
                "sel": {"class": "ロイヤル", "token": True, "kind": "follower"},
                "during": {"turn": "self"},
            }
        },
    ),
    ("BP01-016", "A.event", {"left_field": {"ref": "this"}}),
    (
        "BP11-074",
        "A.event",
        {"left_field": {"who": "self", "sel": {"orig_cost_eq": 1, "token": True}}},
    ),
    (
        "BP18-007",
        "A.event",
        {"left_field": {"ref": "this", "during": {"turn": "self"}}},
    ),
    ("CP04-007", "A.event", {"placed_ex": {"ref": "this"}}),
    (
        "BP21-071",
        "A.event",
        {"placed_ex": {"who": "self", "sel": {"not_class": "ドラゴン"}}},
    ),
    (
        "BP11-008",
        "A.event",
        {
            "placed_ex": {
                "who": "self",
                "sel": {"trait": "乗物"},
                "during": {"turn": "self"},
            }
        },
    ),
    (
        "BP01-170",
        "A.event",
        {"discarded": {"ref": "this", "from": "hand", "who": "self"}},
    ),
    (
        "BP21-043",
        "A.event",
        {
            "discarded": {
                "ref": "this",
                "from": "hand",
                "who": "self",
                "by": {"ability_of": {"who": "self", "trait": "学院"}},
            }
        },
    ),
    ("BP11-070", "A.event", {"discarded": {"during": {"turn": "self"}}}),
    (
        "BP14-074",
        "A.event",
        {
            "any_of": [
                {"discarded": {"ref": "this", "from": "hand", "who": "self"}},
                {"banished": {"ref": "this", "from": "hand", "who": "self"}},
            ]
        },
    ),
    ("BP01-106", "A.event", {"destroyed": {"who": "opp", "sel": {"kind": "follower"}}}),
    ("CP02-SL10", "A.event", {"damaged": {"recipient": "this"}}),
    ("BP21-037", "A.event", {"damaged": {"recipient": "this", "kind": "ability"}}),
    (
        "BP21-109",
        "A.event",
        {"damaged": {"recipient": {"leader": "self"}, "kind": "ability"}},
    ),
    (
        "BP11-049",
        "A.event",
        {"damaged": {"recipient": "this", "during": {"turn": "opp"}}},
    ),
    (
        "BP04-003",
        "A.event",
        {
            "dealt_damage": {
                "ref": "this",
                "kind": "attack",
                "recipient": {"leaders": "opponents"},
            }
        },
    ),
    (
        "CP03-064",
        "A.event",
        {
            "dealt_damage": {
                "who": "self",
                "sel": {"trait": "かげろう"},
                "recipient": {"who": "opp", "zone": "field", "kind": "follower"},
                "kind": "ability",
            }
        },
    ),
    (
        "ECP02-057",
        "A.event",
        {
            "dealt_damage": {
                "who": "self",
                "zone": "field",
                "sel": {"trait": "デレマス"},
                "recipient": {"who": "opp", "zone": "field", "kind": "follower"},
                "during": {"turn": "self"},
            }
        },
    ),
    (
        "BP02-020",
        "A.event",
        {"dealt_damage": {"ref": "this", "kind": "combat", "during": {"turn": "self"}}},
    ),
    (
        "CP03-019",
        "A.event",
        {
            "attacked": {
                "who": "self",
                "sel": {"trait": "アクアフォース", "kind": "follower"},
            }
        },
    ),
    ("BP02-016", "A.event", {"evolved": {"who": "self", "sel": {"kind": "follower"}}}),
    (
        "BP20-004",
        "A.event",
        {
            "evolved": {
                "who": "self",
                "sel": {"orig_cost_le": 5, "trait": "植物族", "kind": "follower"},
            }
        },
    ),
    ("BP05-006", "A.event", {"phase": {"which": "end", "whose": "self"}}),
    ("CP02-044", "A.event", {"played": {"who": "self", "sel": {"orig_cost_ge": 5}}}),
    (
        "BP04-064",
        "A.event",
        {
            "played": {
                "who": "self",
                "sel": {"class": "ドラゴン", "kind": "spell"},
                "during": {"turn": "self"},
            }
        },
    ),
    (
        "BP20-SL05",
        "A.event",
        {
            "any_of": [
                {"fused": {"who": "self", "material": {"trait": "財宝"}}},
                {"played": {"who": "self", "sel": {"trait": "財宝"}}},
            ]
        },
    ),
    ("BP02-095", "A.event", {"life_changed": {"who": "self", "direction": "up"}}),
    (
        "BP19-075",
        "A.event",
        {
            "life_changed": {
                "who": "self",
                "direction": "down",
                "during": {"turn": "self"},
            }
        },
    ),
    (
        "BP11-082",
        "A.event",
        {"stat_changed": {"ref": "this", "stat": ["power", "hp"], "direction": "up"}},
    ),
    ("CP04-102", "A.event", {"acted": {"ref": "this"}}),
    ("CP01-082", "A.event", {"raced": {"who": "self", "sel": {"kind": "follower"}}}),
    (
        "CP04-SL15",
        "A.event",
        {
            "ub_activated": {
                "who": "self",
                "holder": {
                    "who": "self",
                    "zone": "field",
                    "kind": "follower",
                    "other": True,
                },
            }
        },
    ),
    ("BP03-071", "A.event", {"targeted": {"ref": "this"}}),
    ("BP21-SL19", "A.event", {"dice_rolled": {"who": "self"}}),
    (
        "CP03-P27",
        "A.event",
        {
            "drive_checked": {
                "holder": {"who": "self", "zone": "field", "kind": "follower"}
            }
        },
    ),
    ("CP03-P81", "A.event", {"drive_trigger": {"who": "self"}}),
    (
        "BP05-029",
        "A.event",
        {
            "moved": {
                "from": {"who": "opp", "zone": "deck"},
                "to": "cemetery",
                "during": {"turn": "self"},
            }
        },
    ),
    ("BP01-012", "A.event", {"moved": {"ref": "this", "from": "field", "to": "hand"}}),
    (
        "BP06-090",
        "A.event",
        {
            "moved": {
                "from": {"who": "self", "zone": "field"},
                "to": "cemetery",
                "sel": {"keyword": "ward", "kind": "follower"},
            }
        },
    ),
    (
        "BP20-025",
        "A.event",
        {
            "moved": {
                "from": {"who": "opp", "zone": "field"},
                "to": "cemetery",
                "during": {"turn": "self"},
                "sel": {
                    "kind": "follower",
                    "as": "victim",
                    "where": {
                        "history": {
                            "event": "dealt_damage",
                            "window": "this_turn",
                            "where": {"ref": "this", "recipient": "victim"},
                        }
                    },
                },
            }
        },
    ),
    (
        "ECP01-020",
        "A.event",
        {
            "moved": {
                "from": {"who": "opp", "zone": "field"},
                "to": "cemetery",
                "sel": {
                    "kind": "follower",
                    "as": "victim",
                    "where": {
                        "history": {
                            "event": "dealt_damage",
                            "window": "this_turn",
                            "who": "self",
                            "sel": {"trait": "ウマ娘"},
                            "where": {"recipient": "victim"},
                        }
                    },
                },
            }
        },
    ),
    ("BP05-094", "A.event", {"drew": {"who": "self", "not_phase": "start"}}),
    ("CP04-PR22", "A.event", {"equipped": {"equipment": "this"}}),
    (
        "BP19-048",
        "A.event",
        {
            "fused_as_material": {
                "ref": "this",
                "by": {
                    "ability_of": {"who": "self", "kind": "follower", "trait": "八獄"}
                },
            }
        },
    ),
    ("BP05-092", "A.event", {"state": {"leader_life": {"who": "self", "le": 0}}}),
    (
        "BP20-P37",
        "A.cond",
        {
            "history": {
                "event": "damaged",
                "where": {"recipient": "item"},
                "window": "this_turn",
                "at_least": 1,
            }
        },
    ),
    (
        "CP04-P60",
        "A.cond",
        {
            "history": {
                "event": "ub_activated",
                "who": "self",
                "window": "this_turn",
                "exclude": "this_execution",
                "at_least": 2,
            }
        },
    ),
    (
        "BP21-096",
        "A.cond",
        {
            "history": {
                "event": "stat_changed",
                "where": {"ref": "this", "stat": "hp", "direction": "up"},
                "window": "this_turn",
                "at_least": 1,
            }
        },
    ),
    (
        "CP02-007",
        "A.cond",
        {
            "history": {
                "event": "banished",
                "who": "self",
                "from": "ex",
                "sel": {"name": "魔法のアイテム"},
                "window": "this_turn",
            }
        },
    ),
    (
        "BP02-033",
        "A.cond",
        {
            "history": {
                "event": "destroyed",
                "who": "self",
                "sel": {"kind": "follower"},
                "window": "this_turn",
            }
        },
    ),
    (
        "BP21-081",
        "A.cond",
        {
            "history": {
                "event": "dice_rolled",
                "who": "self",
                "where": {"value": {"eq": 6}},
                "window": "this_turn",
            }
        },
    ),
    (
        "BP21-076",
        "A.cond",
        {"history": {"event": "dice_rolled", "who": "self", "window": "this_turn"}},
    ),
    (
        "CP03-007",
        "A.cond",
        {
            "history": {
                "event": "attacked",
                "who": "self",
                "sel": {"trait": "アクアフォース", "kind": "follower"},
                "window": "this_turn",
                "at_least": 3,
            }
        },
    ),
    (
        "BP17-074",
        "A.cond",
        {
            "history": {
                "event": "played",
                "who": "self",
                "sel": {"name": "テラーナイト", "other": True},
                "window": "this_turn",
            }
        },
    ),
    (
        "BP07-005",
        "A.cond",
        {
            "history": {
                "event": "moved",
                "from": {"who": "self", "zone": "field"},
                "to": "cemetery",
                "sel": {"kind": "follower"},
                "window": "this_turn",
            }
        },
    ),
    (
        "BP21-040",
        "A.cond",
        {
            "history": {
                "event": "payment",
                "who": "self",
                "where": {"cost_kind": "earth_rite"},
                "window": "this_turn",
            }
        },
    ),
    (
        "BP14-037",
        "A.cond",
        {
            "cmp": {
                "left": {
                    "history_sum": {
                        "event": "payment",
                        "who": "self",
                        "where": {"cost_kind": "earth_rite"},
                        "field": "removed_stack",
                        "window": "this_turn",
                    }
                },
                "op": "ge",
                "right": 2,
            }
        },
    ),
    (
        "SP01-039",
        "A.cond",
        {
            "cmp": {
                "left": {
                    "history_sum": {
                        "event": "life_changed",
                        "who": "self",
                        "where": {"direction": "up"},
                        "field": "delta",
                        "window": "this_turn",
                    }
                },
                "op": "ge",
                "right": 4,
            }
        },
    ),
    (
        "BP21-092",
        "A.expr.numeric",
        {
            "history_count": {
                "event": "life_changed",
                "who": "self",
                "where": {"direction": "up"},
                "window": "this_turn",
            }
        },
    ),
    (
        "BP01-004",
        "A.expr.numeric",
        {
            "history_count": {
                "event": "played",
                "who": "self",
                "sel": {"other": True},
                "window": "this_turn",
            }
        },
    ),
    (
        "CP02-006",
        "A.cond",
        {
            "cmp": {
                "left": {
                    "at": "event",
                    "value": {
                        "history_count": {
                            "event": "played",
                            "who": "self",
                            "window": "this_turn",
                        }
                    },
                },
                "op": "eq",
                "right": 3,
            }
        },
    ),
    (
        "CP03-005",
        "A.cond",
        {
            "cmp": {
                "left": {
                    "at": "event",
                    "value": {
                        "history_count": {
                            "event": "attacked",
                            "who": "self",
                            "sel": {"trait": "アクアフォース", "kind": "follower"},
                            "window": "this_turn",
                        }
                    },
                },
                "op": "eq",
                "right": 3,
            }
        },
    ),
    ("BP20-018", "A.cond", {"paid": "optional"}),
    ("BP13-098", "A.cond", {"paid": {"receipt": "extra"}}),
    (
        "CP03-SL16",
        "A.cond",
        {
            "did": {
                "result": {"entry_payment": "additional"},
                "where": {"name": "ブラスター・ダーク", "zone": "field"},
            }
        },
    ),
    (
        "BP17-030",
        "A.cond",
        {"chosen": {"ref": "t", "where": {"name_contains": "リオード"}}},
    ),
    ("BP20-073", "A.cond", {"chosen": {"ref": "t", "where": {"hp_le": 3}}}),
    (
        "BP07-088",
        "A.expr.numeric",
        {"counter": {"on": {"lki_of": "this"}, "name": "祈り"}},
    ),
    (
        "BP05-057",
        "A.expr.numeric",
        {"power_of": {"objects_of": "sacrificed", "view": "before"}},
    ),
    (
        "BP04-116",
        "A.expr.numeric",
        {"half_up": {"cost_of": {"objects_of": "discarded", "view": "before"}}},
    ),
    ("BP13-053", "A.expr.numeric", {"plus": [{"orig_cost_of": "t"}, 2]}),
    ("BP12-113", "A.cond", {"cmp": {"left": {"dice": "d"}, "op": "ge", "right": 4}}),
    ("BP03-001", "A.expr.numeric", {"x_paid": "removed"}),
    (
        "BP14-099",
        "A.cond",
        {
            "did": {
                "result": "milled",
                "where": {"not_class": "ビショップ"},
                "view": "after",
            }
        },
    ),
    (
        "ECP01-036",
        "A.cond",
        {"did": {"result": "cost", "where": {"trait": "ウマ娘", "orig_cost_ge": 7}}},
    ),
    (
        "BP08-103",
        "A.expr.numeric",
        {
            "count_of": {
                "receipt": "milled",
                "where": {"orig_cost_eq": {"bound": "declared"}},
                "view": "after",
            }
        },
    ),
    (
        "BP03-045",
        "A.expr.numeric",
        {
            "count_of": {
                "receipt": "milled",
                "where": {"kind": "spell"},
                "view": "after",
            }
        },
    ),
    (
        "ECP01-045",
        "A.expr.numeric",
        {
            "count_of": {
                "receipt": "milled",
                "where": {"trait": "ウマ娘"},
                "view": "after",
            }
        },
    ),
    (
        "CP02-006",
        "A.expr.numeric",
        {
            "at": "event",
            "value": {
                "history_count": {
                    "event": "played",
                    "who": "self",
                    "window": "this_turn",
                }
            },
        },
    ),
    (
        "CP01-029",
        "A.expr.numeric",
        {
            "at": "event",
            "value": {
                "history_count": {
                    "event": "played",
                    "who": "self",
                    "sel": {"kind": "spell"},
                    "window": "this_turn",
                }
            },
        },
    ),
    (
        "CP03-005",
        "A.expr.numeric",
        {
            "at": "event",
            "value": {
                "history_count": {
                    "event": "attacked",
                    "who": "self",
                    "window": "this_turn",
                    "sel": {"kind": "follower", "trait": "アクアフォース"},
                }
            },
        },
    ),
    (
        "BP19-075",
        "A.cond",
        {
            "history": {
                "event": "life_changed",
                "who": "self",
                "where": {"direction": "down"},
                "window": "this_turn",
                "at_least": 4,
            }
        },
    ),
    (
        "SP01-039",
        "A.expr.numeric",
        {
            "history_sum": {
                "event": "life_changed",
                "who": "self",
                "where": {"direction": "up"},
                "window": "this_turn",
                "field": "delta",
            }
        },
    ),
    (
        "BP14-037",
        "A.expr.numeric",
        {
            "history_sum": {
                "event": "payment",
                "who": "self",
                "where": {"cost_kind": "earth_rite"},
                "window": "this_turn",
                "field": "removed_stack",
            }
        },
    ),
    (
        "CP04-003",
        "A.expr.numeric",
        {
            "history_count": {
                "event": "ub_activated",
                "who": "self",
                "window": "this_turn",
                "exclude": "this_execution",
            }
        },
    ),
    (
        "BP10-102",
        "A.expr.numeric",
        {"counter": {"on": {"lki_of": "this"}, "name": "災い"}},
    ),
    (
        "BP17-008",
        "A.expr.numeric",
        {"counter": {"on": {"lki_of": "this"}, "name": "矢"}},
    ),
    (
        "BP05-057",
        "A.expr.numeric",
        {"power_of": {"objects_of": "cost", "view": "before"}},
    ),
    (
        "BP04-116",
        "A.expr.numeric",
        {"cost_of": {"objects_of": "cost", "view": "before"}},
    ),
    ("BP13-053", "A.expr.numeric", {"orig_cost_of": "target"}),
    (
        "BP17-030",
        "A.cond",
        {"chosen": {"ref": "target", "where": {"name_contains": "リオード"}}},
    ),
    ("BP20-073", "A.cond", {"chosen": {"ref": "target", "where": {"hp_le": 3}}}),
    ("BP20-P12", "A.cond", {"paid": "optional"}),
    (
        "BP06-111",
        "A.cond",
        {"cmp": {"left": {"dice": "roll"}, "op": "eq", "right": {"bound": "declared"}}},
    ),
    (
        "BP17-045",
        "A.cond",
        {
            "did": {
                "result": "milled",
                "where": {"not_class": "ウィッチ"},
                "view": "after",
            }
        },
    ),
    ("CP02-SL10-amount", "A.expr.numeric", {"event": "amount"}),
    (
        "BP20-025",
        "A.event",
        {
            "moved": {
                "from": {"who": "opp", "zone": "field"},
                "to": "cemetery",
                "during": {"turn": "self"},
                "sel": {
                    "kind": "follower",
                    "where": {
                        "history": {
                            "event": "dealt_damage",
                            "where": {"ref": "this", "recipient": "item"},
                            "window": "this_turn",
                        }
                    },
                },
            }
        },
    ),
    (
        "ECP01-020",
        "A.cond",
        {
            "history": {
                "event": "dealt_damage",
                "who": "self",
                "sel": {"trait": "ウマ娘"},
                "where": {"recipient": "item"},
                "window": "this_turn",
            }
        },
    ),
    (
        "BP16-018",
        "A.cond",
        {
            "history": {
                "event": "evolved",
                "who": "self",
                "sel": {"kind": "follower"},
                "window": "this_turn",
            }
        },
    ),
    ("BP04-108", "A.event", {"left_field": {"who": "self", "sel": {"kind": "amulet"}}}),
    (
        "BP14-027",
        "A.event",
        {"placed_ex": {"who": "self", "sel": {"name": "輝く金貨"}}},
    ),
    (
        "BP09-006",
        "A.event",
        {
            "discarded": {
                "who": "self",
                "from": "hand",
                "sel": {"class": "エルフ", "kind": "spell"},
            }
        },
    ),
    (
        "BP03-034",
        "A.event",
        {
            "destroyed": {
                "who": "opp",
                "sel": {"kind": "follower"},
                "during": {"turn": "self"},
            }
        },
    ),
    (
        "BP12-008",
        "A.event",
        {
            "attacked": {
                "who": "self",
                "sel": {"other": True, "trait": "狩人", "kind": "follower"},
            }
        },
    ),
    ("BP01-153", "A.event", {"phase": {"which": "end", "whose": "self"}}),
    (
        "BP14-036",
        "A.event",
        {
            "played": {
                "who": "self",
                "sel": {"orig_cost_le": 3, "traits_any": ["宴楽", "魔法使い"]},
            }
        },
    ),
    (
        "BP21-100",
        "A.event",
        {"stat_changed": {"ref": "this", "stat": "hp", "direction": "up"}},
    ),
    ("BP11-069", "A.event", {"acted": {"ref": "this"}}),
    (
        "CP01-032",
        "A.event",
        {"raced": {"who": "self", "sel": {"other": True, "kind": "follower"}}},
    ),
    (
        "BP20-029",
        "A.event",
        {
            "any_of": [
                {"fused": {"who": "self", "material": {"trait": "財宝"}}},
                {"played": {"who": "self", "sel": {"trait": "財宝"}}},
            ]
        },
    ),
    (
        "CP03-065",
        "A.event",
        {
            "drive_checked": {
                "holder": {"who": "self", "zone": "field", "kind": "follower"}
            }
        },
    ),
    ("CP03-110", "A.event", {"drive_trigger": {"who": "self"}}),
    ("BP21-075", "A.expr.numeric", {"event": "value"}),
    ("BP11-068", "A.cond", {"paid": "optional"}),
    ("BP10-020", "A.cond", {"cmp": {"left": {"dice": "d"}, "op": "le", "right": 2}}),
    ("BP12-114", "A.expr.numeric", {"orig_cost_of": "t"}),
]
