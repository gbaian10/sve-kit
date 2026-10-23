"""Payment node adapters and card-derived fragments, independent of schema generation."""

from typing import TYPE_CHECKING

from sve_carddb.contracts import author_costs as costs

from .author_adapters import adapters

if TYPE_CHECKING:
    from pydantic import JsonValue

COST_NAMES = {
    "A.play.conditional-delta": "ConditionalCostDelta",
    "A.play.conditional-set": "ConditionalCostSet",
    "A.play.cost-delta": "PlayCostDelta",
    "A.play.cost-set": "PlayCostSet",
    "A.play.cost-deltas": "PlayCostDeltas",
    "A.play.cost-sets": "PlayCostSets",
    "A.cost.material-group.exact": "MaterialGroupExact",
    "A.cost.material.exact": "MaterialExact",
    "A.cost.material-group.up-to": "MaterialGroupUpTo",
    "A.cost.material.up-to": "MaterialUpTo",
    "A.cost.material-group.any": "MaterialGroupAny",
    "A.cost.material.any": "MaterialAny",
    "A.cost.material-group": "MaterialGroup",
    "A.cost.material.groups": "MaterialGroups",
    "A.cost.material": "Material",
    "A.cost.discard-random": "RandomDiscardMaterial",
    "A.cost.discard-ref": "DiscardRef",
    "A.cost.banish-top": "BanishTop",
    "A.cost.shuffle-material.exact": "ShuffleMaterialExact",
    "A.cost.shuffle-material.up-to": "ShuffleMaterialUpTo",
    "A.cost.shuffle-material.any": "ShuffleMaterialAny",
    "A.cost.shuffle-material.groups": "ShuffleMaterialGroups",
    "A.cost.shuffle-material": "ShuffleMaterial",
    "A.cost.skip-turn": "SkipTurn",
    "A.cost.step": "CostStep",
    "A.cost": "Cost",
    "A.cost.alternative": "CostAlternative",
    "A.cost.additional": "AdditionalCost",
    "A.cost.optional": "OptionalCost",
    "A.cost.optional-mode": "OptionalModeCost",
    "A.cost.optionals": "OptionalCosts",
    "A.cost.alt": "AltCost",
    "A.x.free": "FreeX",
    "A.x.payment": "PaymentX",
    "A.x": "XParameter",
    "A.play.costs": "PlayCosts",
}


CARD_COSTS: list[tuple[str, dict[str, JsonValue]]] = [
    ("BP03-001", {"pp": 1}),
    ("BP01-105", {"leader_life": 3}),
    ("CP02-003", {"lesson": 1}),
    ("BP01-051", {"earth_rite": 1}),
    ("BP06-058", {"pp_max": 1}),
    ("BP01-001", {"act_self": True}),
    ("BP07-067", {"self_to_cemetery": True}),
    ("BP08-041", {"banish_self": True}),
    (
        "BP03-045",
        {"discard": {"sel": {"who": "self", "zone": "hand", "kind": "spell"}, "n": 1}},
    ),
    (
        "BP14-023",
        {"banish": {"sel": {"who": "self", "zone": "ex", "name": "輝く金貨"}, "n": 2}},
    ),
    (
        "BP01-SL15",
        {
            "to_cemetery": {
                "sel": {
                    "who": "self",
                    "zone": "field",
                    "acted": False,
                    "class": "ナイトメア",
                },
                "n": 4,
            }
        },
    ),
    (
        "BP01-002",
        {
            "return_to_hand": {
                "sel": {"who": "self", "zone": "field", "other": True},
                "n": 1,
            }
        },
    ),
    (
        "BP04-022",
        {
            "reveal": {
                "sel": {
                    "any_of": [
                        {"who": "self", "zone": "hand", "trait": "指揮官"},
                        {"who": "self", "zone": "hand", "trait": "円卓"},
                    ]
                },
                "n": 2,
            }
        },
    ),
    (
        "CP03-029",
        {
            "flip_down": {
                "sel": {
                    "who": "self",
                    "zone": "evolve_deck",
                    "face_up": True,
                    "kind": "evolve_follower",
                    "name_contains": "ブラスター",
                },
                "n": 1,
            }
        },
    ),
    (
        "BP08-053",
        {
            "flip_up": {
                "sel": {
                    "who": "self",
                    "zone": "evolve_deck",
                    "face_up": False,
                    "kind": "evolve_follower",
                },
                "n": 3,
            }
        },
    ),
    (
        "CP04-104",
        {"act": {"sel": {"who": "self", "zone": "field", "kind": "amulet"}, "n": "X"}},
    ),
    ("BP08-072", {"to_ex": {"sel": {"in": "this"}, "n": 1}}),
    ("BP07-012", {"to_deck_bottom": {"sel": {"in": "this"}, "n": 1}}),
    (
        "BP05-067",
        {
            "to_deck": {
                "sel": {"who": "self", "zone": "cemetery"},
                "n": 5,
                "shuffle": True,
            }
        },
    ),
    ("BP01-062", {"counter": {"on": "this", "name": "スペル", "n": 3}}),
    ("BP05-086", {"skip_turn": {"who": "self"}}),
    (
        "BP01-SL15",
        {
            "one_of": [
                {
                    "to_cemetery": {
                        "sel": {
                            "who": "self",
                            "zone": "field",
                            "acted": False,
                            "class": "ナイトメア",
                        },
                        "n": 4,
                    }
                },
                {
                    "banish": {
                        "sel": {"who": "self", "zone": "ex", "class": "ナイトメア"},
                        "n": 4,
                    }
                },
            ]
        },
    ),
    (
        "BP01-016",
        {"steps": [{"pp": 1}, {"act_self": True}, {"self_to_cemetery": True}]},
    ),
]

CARD_WRAPPERS: list[tuple[str, str, JsonValue]] = [
    (
        "BP10-019",
        "A.cost.optional",
        {"steps": [{"pp": 4}, {"self_to_cemetery": True}], "option": 4},
    ),
    ("BP04-066", "A.cost.optional", {"pp": 5, "result": "boosted"}),
    (
        "BP19-090",
        "A.cost.optionals",
        [
            {"pp": 1, "option": 1, "result": "first_paid"},
            {"pp": 1, "option": 2, "result": "second_paid"},
        ],
    ),
    (
        "BP17-116",
        "A.cost.additional",
        {
            "flip_up": {
                "sel": {
                    "who": "self",
                    "zone": "evolve_deck",
                    "face_up": False,
                    "kind": "evolve_follower",
                },
                "n": 1,
            }
        },
    ),
    (
        "EBD02-007",
        "A.play.costs",
        {
            "optional_cost": {
                "banish": {
                    "sel": {"who": "self", "zone": "cemetery", "kind": "spell"},
                    "n": 10,
                }
            },
            "optional_cost_set": 7,
        },
    ),
    ("BP03-001", "A.x", {"from_payment": "removed"}),
    ("CP04-104", "A.x", {}),
]

CARD_TARGETS: list[tuple[str, JsonValue]] = [
    (
        "BP19-080",
        {
            "bind": "t",
            "from": {
                "who": "self",
                "zone": "cemetery",
                "kind": "follower",
                "trait": "八獄",
            },
            "groups": [{"orig_cost_le": 5}, {"orig_cost_le": 3}],
            "upto": 2,
        },
    ),
    (
        "ECP02-029",
        {
            "bind": "t",
            "from": {
                "who": "self",
                "zone": "cemetery",
                "kind": "follower",
                "trait": "パッション",
            },
            "groups": [{"orig_cost_eq": 2}, {"orig_cost_eq": 1}],
            "upto": 2,
        },
    ),
    (
        "BP03-040",
        {
            "bind": "t",
            "from": {
                "who": "self",
                "zone": "cemetery",
                "cost_le": 4,
                "trait": "チェス",
                "kind": "follower",
            },
            "upto": 4,
            "distinct_by": "name",
        },
    ),
    (
        "BP08-104",
        {
            "bind": "t",
            "from": {"who": "self", "zone": "cemetery"},
            "n": 3,
            "distinct_by": "name",
        },
    ),
]

CARD_WRAPPERS.append(
    (
        "CP03-083",
        "A.cost.alt",
        {
            "if": {
                "cmp": {
                    "left": {
                        "count": {
                            "who": "self",
                            "zone": "cemetery",
                            "trait": "シャドウパラディン",
                        }
                    },
                    "op": "ge",
                    "right": 15,
                }
            },
            "may": True,
            "cost": {
                "to_cemetery": {
                    "sel": {
                        "who": "self",
                        "zone": "field",
                        "name": "ファントム・ブラスター・ドラゴン",
                    },
                    "n": 1,
                }
            },
        },
    )
)

COSTS = adapters(
    {key: (getattr(costs, name), name) for key, name in COST_NAMES.items()}
)
