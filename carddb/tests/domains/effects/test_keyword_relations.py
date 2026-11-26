"""Keyword relations follow who receives a keyword and how a filter is used."""

import pytest
from pydantic import JsonValue

from sve_carddb.domains.effects.keyword_relations import (
    UnclassifiedKeywordError,
    ability_relations,
    program_relations,
)

_WARD: dict[str, JsonValue] = {"keyword": "ward", "kind": "follower", "who": "self"}
_ALLIES: dict[str, JsonValue] = {"kind": "follower", "who": "self", "zone": "field"}


def grant(recipient: JsonValue, *keywords: str) -> dict[str, JsonValue]:
    return {"grant": {"keywords": list[JsonValue](keywords), "on": recipient}}


@pytest.mark.parametrize(
    ("ability", "expected"),
    [
        ({"keywords": ["ward", "storm"]}, {("ward", "has"), ("storm", "has")}),
        ({"fanfare": {"do": [grant("this", "storm")]}}, {("storm", "has")}),
        (
            {"static": {"effects": [{"grant": {"keywords": ["rush"]}}]}},
            {("rush", "has")},
        ),
        (
            {
                "static": {
                    "effects": [
                        {
                            "if": {
                                "cond": {"exists": _WARD},
                                "then": [{"grant": {"keywords": ["bane"]}}],
                            }
                        }
                    ]
                }
            },
            {("bane", "has"), ("ward", "refers")},
        ),
        (
            {"static": {"effects": [{"keywords": {"list": ["aura"]}}]}},
            {("aura", "has")},
        ),
        (
            {
                "lastword": {
                    "do": [
                        {"move": {"ref": "this", "result": "back", "to": "field"}},
                        grant({"follow": "back"}, "storm"),
                    ]
                }
            },
            {("storm", "has")},
        ),
        # A result of acting on an earlier self result is still this card.
        (
            {
                "lastword": {
                    "do": [
                        {"move": {"ref": "this", "result": "back", "to": "field"}},
                        {"evolve": {"ref": {"follow": "back"}, "result": "changed"}},
                        grant("changed", "storm"),
                    ]
                }
            },
            {("storm", "has")},
        ),
        (
            {
                "lastword": {
                    "do": [
                        {
                            "move": {
                                "ref": "this",
                                "to": "field",
                                "entry_effects": [{"grant": {"keywords": ["rush"]}}],
                            }
                        }
                    ]
                }
            },
            {("rush", "has")},
        ),
        # A transformed card is another object, whatever it was before.
        (
            {
                "fanfare": {
                    "do": [
                        {
                            "transform": {
                                "ref": "this",
                                "into": "Synthetic",
                                "result": "form",
                                "entry_effects": [{"grant": {"keywords": ["ward"]}}],
                            }
                        },
                        grant("form", "storm"),
                    ]
                }
            },
            {("ward", "grants"), ("storm", "grants")},
        ),
        # A name bound inside a granted ability does not alias this card outside it.
        (
            {
                "fanfare": {
                    "do": [
                        {
                            "grant": {
                                "on": "ally",
                                "abilities": [
                                    {
                                        "lastword": {
                                            "do": [
                                                {
                                                    "move": {
                                                        "ref": "this",
                                                        "result": "moved",
                                                        "to": "field",
                                                    }
                                                }
                                            ]
                                        }
                                    }
                                ],
                            }
                        },
                        {"choose": {"bind": "moved", "from": _ALLIES}},
                        grant("moved", "bane"),
                    ]
                }
            },
            {("bane", "grants")},
        ),
        ({"fanfare": {"do": [grant("ally", "rush")]}}, {("rush", "grants")}),
        ({"auto": {"do": [grant("it", "ward")]}}, {("ward", "grants")}),
        ({"static": {"effects": [grant(_ALLIES, "storm")]}}, {("storm", "grants")}),
        (
            {
                "fanfare": {
                    "do": [
                        {
                            "apply": {
                                "on": "ally",
                                "effect": {"keywords": {"list": ["drain"]}},
                            }
                        }
                    ]
                }
            },
            {("drain", "grants")},
        ),
        (
            {
                "fanfare": {
                    "do": [
                        {
                            "apply": {
                                "on": "this",
                                "effect": {"grant": {"keywords": ["drain"]}},
                            }
                        }
                    ]
                }
            },
            {("drain", "has")},
        ),
        (
            {
                "fanfare": {
                    "do": [
                        {
                            "create": {
                                "token": "Synthetic",
                                "to": "field",
                                "entry_effects": [{"grant": {"keywords": ["rush"]}}],
                            }
                        }
                    ]
                }
            },
            {("rush", "grants")},
        ),
        (
            {
                "fanfare": {
                    "do": [
                        {
                            "grant": {
                                "keywords": ["rush"],
                                "on": "ally",
                                "instead": [
                                    {"if": {"exists": _WARD}, "keywords": ["storm"]}
                                ],
                            }
                        }
                    ]
                }
            },
            {("rush", "grants"), ("storm", "grants"), ("ward", "refers")},
        ),
        # Inside an ability given to another card, "this" is that card.
        (
            {
                "fanfare": {
                    "do": [
                        {
                            "grant": {
                                "on": "ally",
                                "abilities": [
                                    {"attack_time": {"do": [grant("this", "bane")]}}
                                ],
                            }
                        }
                    ]
                }
            },
            {("bane", "grants")},
        ),
        (
            {
                "fanfare": {
                    "do": [
                        {
                            "grant": {
                                "on": "this",
                                "abilities": [
                                    {"attack_time": {"do": [grant("this", "bane")]}}
                                ],
                            }
                        }
                    ]
                }
            },
            {("bane", "has")},
        ),
        (
            {"fanfare": {"do": [{"buff": {"on": _WARD, "power": 1}}]}},
            {("ward", "refers")},
        ),
        (
            {"fanfare": {"do": [{"buff": {"on": "this", "power": {"count": _WARD}}}]}},
            {("ward", "counts")},
        ),
        (
            {
                "fanfare": {
                    "do": [
                        {
                            "buff": {
                                "on": "this",
                                "power": {"count": {"any_of": [_WARD, _ALLIES]}},
                            }
                        }
                    ]
                }
            },
            {("ward", "counts")},
        ),
        (
            {"static": {"effects": [{"ignore_ward": {"on": "this"}}]}},
            {("ward", "refers")},
        ),
        # An unclear recipient only matters when a tracked keyword is granted.
        (
            {
                "static": {
                    "zone": "equipment",
                    "effects": [{"grant": {"abilities": [{"fanfare": {"do": []}}]}}],
                }
            },
            set(),
        ),
        # Concepts outside the tracked set keep their own handling.
        (
            {
                "fanfare": {
                    "do": [
                        {"search": {"sel": {"keyword": "quick_spell"}, "to": "hand"}},
                        grant("ally", "quick"),
                    ]
                }
            },
            set(),
        ),
    ],
)
def test_each_use_maps_to_one_relation(
    ability: dict[str, JsonValue], expected: set[tuple[str, str]]
) -> None:
    assert ability_relations(ability | {"line": 1, "ports": {}}) == expected


@pytest.mark.parametrize(
    "ability",
    [
        # The recipient of an omitted "on" is unclear for a player effect or an equipment.
        {
            "fanfare": {
                "do": [
                    {
                        "player_effect": {
                            "who": "self",
                            "effect": {"grant": {"keywords": ["ward"]}},
                        }
                    }
                ]
            }
        },
        {
            "static": {
                "zone": "equipment",
                "effects": [{"grant": {"keywords": ["ward"]}}],
            }
        },
        {"fanfare": {"do": [{"future_step": {"gain": ["ward"]}}]}},
        {"fanfare": {"do": [{"remove_ward": {"on": "ally"}}]}},
    ],
    ids=["player-effect", "equipment", "unknown-value", "unknown-key"],
)
def test_unknown_forms_fail_instead_of_reading_as_checked(
    ability: dict[str, JsonValue],
) -> None:
    with pytest.raises(UnclassifiedKeywordError):
        ability_relations(ability)


def test_program_unions_card_and_face_abilities() -> None:
    program: dict[str, JsonValue] = {
        "abilities": [{"keywords": ["ward"]}],
        "faces": {
            "f:front": {"abilities": [{"fanfare": {"do": [grant("ally", "rush")]}}]},
            "f:back": {"abilities": [{"keywords": ["storm"]}]},
        },
    }
    assert program_relations(program) == {
        ("ward", "has"),
        ("rush", "grants"),
        ("storm", "has"),
    }
