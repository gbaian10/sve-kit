"""Card-labelled query fragments from frozen JP text and sections, not full effects."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pydantic import JsonValue

CARD_QUERIES: list[tuple[str, str, JsonValue]] = [
    (
        "BP09-077",
        "A.sel.object-core",
        {"who": "current", "zone": "field", "kind": "follower", "power_max": True},
    ),
    (
        "ECP02-072",
        "A.sel.object-core",
        {"who": "each", "zone": "field", "kind": "follower", "not_trait": "デレマス"},
    ),
    (
        "BP13-051",
        "A.sel.object-core",
        {
            "who": "self",
            "zone": "cemetery",
            "kind": "follower",
            "orig_cost_le": 2,
            "traits_any": ["獣", "魔法生物"],
        },
    ),
    (
        "SP01-043",
        "A.sel.object-core",
        {
            "who": "self",
            "zone": "deck",
            "kind": "follower",
            "class": "ビショップ",
            "not_same_name_as_this": True,
            "orig_cost_le": {
                "minus": [{"resource": {"who": "self", "kind": "pp_max"}}, 1]
            },
        },
    ),
    (
        "BP03-112",
        "A.sel.object-core",
        {
            "who": "self",
            "zone": "hand",
            "kind": "follower",
            "class": "ニュートラル",
            "cost_le": {"resource": {"who": "self", "kind": "pp_max"}},
        },
    ),
    (
        "BP09-001",
        "A.sel.object-core",
        {
            "who": "self",
            "zone": "cemetery",
            "class": "エルフ",
            "kind": "spell",
            "orig_cost_le": {
                "distinct_names": {
                    "who": "self",
                    "zone": "cemetery",
                    "class": "エルフ",
                    "kind": "spell",
                }
            },
        },
    ),
    (
        "CP03-123",
        "A.sel.object-core",
        {"who": "self", "zone": "cemetery", "kind": "follower", "has_trigger": True},
    ),
    (
        "BP11-024",
        "A.sel.object-core",
        {"who": "opp", "zone": "field", "kind": "follower", "boxed": True},
    ),
    (
        "BP02-041",
        "A.sel.object-core",
        {"who": "opp", "zone": "field", "kind": "follower", "evolved": False},
    ),
    (
        "BP02-101",
        "A.sel.object-core",
        {
            "who": "self",
            "zone": "field",
            "kind": "follower",
            "other": True,
            "entered_this_turn": True,
        },
    ),
    (
        "BP08-053",
        "A.sel.object-core",
        {"who": "self", "zone": "evolve_deck", "face_up": False},
    ),
    (
        "BP04-070",
        "A.sel.object-core",
        {
            "who": "opp",
            "zone": "field",
            "kind": "follower",
            "where": {
                "cmp": {
                    "left": {"cost_of": "item"},
                    "op": "eq",
                    "right": {
                        "agg": {
                            "of": {"who": "opp", "zone": "field", "kind": "follower"},
                            "attr": "cost",
                            "fn": "min",
                        }
                    },
                }
            },
        },
    ),
    (
        "BP02-105",
        "A.expr.numeric",
        {
            "count": {
                "who": "self",
                "zone": "field",
                "kind": "follower",
                "keyword": "ward",
                "other": True,
            }
        },
    ),
    (
        "BP06-018",
        "A.expr.numeric",
        {"count": {"who": "self", "zone": "evolve_deck", "face_up": True}},
    ),
    (
        "BP12-034",
        "A.expr.numeric",
        {
            "agg": {
                "of": {"who": "self", "zone": "field", "kind": "follower"},
                "attr": "power",
                "fn": "max",
            }
        },
    ),
    (
        "BP10-011",
        "A.expr.numeric",
        {"distinct_orig_costs": {"who": "self", "zone": "ex"}},
    ),
    ("BP13-038", "A.expr.numeric", {"counter": {"on": "this", "name": "魔力"}}),
    ("ECP02-020", "A.expr.numeric", {"hand": {"master_of": "t"}}),
    (
        "BP04-034",
        "A.cond",
        {
            "exists": {
                "who": "self",
                "zone": "field",
                "kind": "follower",
                "not_class": "ロイヤル",
            }
        },
    ),
    (
        "BP09-117",
        "A.cond",
        {
            "cmp": {
                "left": {
                    "count": {
                        "who": "self",
                        "zone": "field",
                        "kind": "follower",
                        "traits_any": ["天使", "堕天使"],
                    }
                },
                "op": "ge",
                "right": 2,
            }
        },
    ),
    (
        "CP04-022",
        "A.cond",
        {
            "cmp": {
                "left": {
                    "agg": {
                        "of": {
                            "who": "self",
                            "zone": "field",
                            "kind": "follower",
                            "orig_cost_eq": 1,
                        },
                        "attr": "power",
                        "fn": "sum",
                    }
                },
                "op": "ge",
                "right": 10,
            }
        },
    ),
    (
        "ECP02-005",
        "A.cond",
        {
            "cmp": {
                "left": {
                    "agg": {
                        "of": {"who": "self", "zone": "field", "trait": "パッション"},
                        "attr": "orig_cost",
                        "fn": "sum",
                    }
                },
                "op": "ge",
                "right": 10,
            }
        },
    ),
    (
        "BP21-060",
        "A.cond",
        {
            "exists": {
                "who": "self",
                "zone": "ex",
                "where": {
                    "cmp": {
                        "left": {"counter": {"on": "item", "name": "情熱"}},
                        "op": "ge",
                        "right": 4,
                    }
                },
            }
        },
    ),
    ("BP03-028", "A.cond", {"not": {"entered_from": {"ref": "this", "from": "hand"}}}),
    ("BP10-062", "A.cond", {"played_from": {"ref": "this", "from": "ex"}}),
    ("BP17-SL25", "A.cond", {"entered_by": {"ref": "this", "by": "ability"}}),
    ("BP01-SL16", "A.cond", {"is": {"ref": "attack_target", "kind": "follower"}}),
    ("BP18-073", "A.cond", {"on_field": "this"}),
    ("BP07-058", "A.cond", {"in_zone": {"ref": "this", "zone": "ex"}}),
    ("BP15-102", "A.cond", {"standing": "this"}),
    ("CP03-003", "A.cond", {"my_turn": True}),
    ("CP02-T01", "A.cond", {"construction": "アイドルマスター シンデレラガールズ"}),
    ("BP01-014", "A.cond", {"combo": 3}),
    ("ETD01-018", "A.cond", {"awakened": True}),
    ("BP14-072", "A.cond", {"hand": {"who": "self", "eq": 0}}),
    ("BP15-076", "A.cond", {"leader_life": {"who": "self", "eq": 7}}),
    ("BP13-043", "A.cond", {"zone_free": {"who": "current", "zone": "field", "ge": 1}}),
    (
        "BP03-092",
        "A.sel.object-core",
        {
            "any_of": [
                {
                    "who": "self",
                    "zone": "cemetery",
                    "kind": "follower",
                    "cost_le": 3,
                    "keyword": "storm",
                },
                {
                    "who": "self",
                    "zone": "cemetery",
                    "kind": "follower",
                    "cost_le": 3,
                    "keyword": "ward",
                },
            ]
        },
    ),
    (
        "BP01-039",
        "A.sel.object-core",
        {"in": "looked", "kind": "follower", "cost_eq": 1},
    ),
    (
        "BP01-053",
        "A.sel.object-core",
        {"who": "self", "zone": "cemetery", "kind": "spell", "cost_le": 3},
    ),
    (
        "BP01-100",
        "A.sel.object-core",
        {"in": "looked", "class": "ドラゴン", "cost_ge": 5},
    ),
    (
        "BP05-042",
        "A.sel.object-core",
        {"who": "self", "zone": "deck", "orig_cost_eq": 1},
    ),
    (
        "BP02-115",
        "A.sel.object-core",
        {"who": "opp", "zone": "field", "kind": "follower", "orig_cost_le": 2},
    ),
    (
        "BP05-024",
        "A.cond",
        {
            "exists": {
                "who": "opp",
                "zone": "field",
                "kind": "follower",
                "orig_cost_ge": 6,
            }
        },
    ),
    (
        "BP01-066",
        "A.sel.object-core",
        {"who": "opp", "zone": "field", "kind": "follower", "hp_le": 4},
    ),
    (
        "BP01-035",
        "A.sel.object-core",
        {"who": "self", "zone": "deck", "kind": "follower", "has_evolve": True},
    ),
    ("BP01-065", "A.cond", {"spellchain": 10}),
    ("BP02-080", "A.cond", {"necro": 10}),
    ("BP01-112", "A.cond", {"crimson": True}),
    ("BP03-020", "A.cond", {"hand": {"who": "self", "le": 2}}),
    ("BP05-096", "A.cond", {"hand": {"who": "self", "ge": 5}}),
    ("BP10-109", "A.cond", {"leader_life": {"who": "self", "le": 10}}),
    ("BP03-090", "A.cond", {"entered_from": {"ref": "this", "from": "hand"}}),
    ("BP07-038", "A.cond", {"entered_from": {"ref": "this", "from": "cemetery"}}),
]
