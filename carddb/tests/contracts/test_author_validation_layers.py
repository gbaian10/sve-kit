"""Author syntax acceptance is independent of backend execution admission."""

from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator

from sve_carddb.core.json import array, object_value, parse, string

if TYPE_CHECKING:
    from pydantic import JsonValue


SCHEMAS = Path(__file__).resolve().parents[3] / "dsl"
NODES = object_value(parse((SCHEMAS / "author-nodes.schema.json").read_bytes()))


@pytest.mark.parametrize(
    ("head", "accepted"),
    [
        pytest.param(
            {
                "spell": {
                    "do": [
                        {
                            "create": {
                                "token": "synthetic-token",
                                "to": "field",
                                "n": {
                                    "per": {
                                        "of": {
                                            "count": {"who": "self", "zone": "cemetery"}
                                        },
                                        "unit": 5,
                                    }
                                },
                            }
                        }
                    ]
                }
            },
            True,
            id="token-per-public-count",
        ),
        pytest.param(
            {
                "spell": {
                    "do": [
                        {
                            "create": {
                                "token": "synthetic-token",
                                "to": "field",
                                "n": {"per": {"of": 5, "unit": 0}},
                            }
                        }
                    ]
                }
            },
            False,
            id="token-per-zero-unit",
        ),
        pytest.param(
            {
                "spell": {
                    "do": [
                        {
                            "create": {
                                "token": "synthetic-token",
                                "to": "field",
                                "n": {"div": [5, 2]},
                            }
                        }
                    ]
                }
            },
            False,
            id="unknown-division-spelling",
        ),
    ],
)
def test_author_syntax_acceptance_at_consumer_sites(
    head: JsonValue, *, accepted: bool
) -> None:
    _assert_acceptance(head, accepted=accepted)


def _assert_acceptance(
    node: JsonValue, *, accepted: bool, definition: str = "A.ability-head"
) -> None:
    schema = {
        "$id": NODES["$id"],
        "$defs": NODES["$defs"],
        "$ref": f"#/$defs/{definition}",
    }
    assert Draft202012Validator(schema).is_valid(node) is accepted


@pytest.mark.parametrize(
    ("branch", "accepted"),
    [
        pytest.param(
            {"cond": {"my_turn": True}, "then": [{"grant": {"keywords": ["ward"]}}]},
            True,
            id="one-grant",
        ),
        pytest.param(
            {
                "cond": {"my_turn": True},
                "then": [
                    {
                        "grant": {
                            "abilities": [{"lastword": {"do": [{"draw": {"n": 1}}]}}]
                        }
                    }
                ],
            },
            True,
            id="one-granted-body",
        ),
        pytest.param(
            {"cond": {"my_turn": True}, "then": [{"grant": {}}]},
            False,
            id="empty-grant",
        ),
    ],
)
def test_conditional_static_declarations_are_closed(
    branch: JsonValue, *, accepted: bool
) -> None:
    _assert_acceptance({"static": {"effects": [{"if": branch}]}}, accepted=accepted)


def test_conditional_grants_preserve_the_head_and_stay_out_of_installed_effects() -> (
    None
):
    declaration: JsonValue = {
        "if": {"cond": {"my_turn": True}, "then": [{"grant": {"keywords": ["ward"]}}]}
    }
    _assert_acceptance(
        {"static": {"while": {"awakened": True}, "effects": [declaration]}},
        accepted=True,
    )
    _assert_acceptance(
        {"spell": {"do": [{"apply": {"on": "this", "effect": declaration}}]}},
        accepted=False,
    )


@pytest.mark.parametrize(
    ("definition", "control", "invalid"),
    [
        pytest.param(
            "A.expr.count-objects",
            {
                "who": "self",
                "zone": "field",
                "as": "counted",
                "where": {"my_turn": True},
            },
            {"who": "self", "zone": "field", "as": "counted"},
            id="alias-requires-where",
        ),
        pytest.param(
            "A.history.destroyed-condition",
            {"event": "destroyed", "window": "this_turn", "sel": {"kind": "follower"}},
            {
                "event": "destroyed",
                "window": "this_turn",
                "sel": {"kind": "follower"},
                "where": {"ref": "this"},
            },
            id="history-subject-is-exclusive",
        ),
        pytest.param(
            "A.effect.card-predicate",
            {"name": "sample"},
            {"name": "sample", "names": ["sample"]},
            id="name-filters-are-exclusive",
        ),
        pytest.param(
            "A.head.play",
            {"optional_cost": {"pp": 1}, "optional_cost_delta": -1},
            {
                "optional_cost": [{"pp": 1, "option": 1}, {"pp": 1, "option": 2}],
                "optional_cost_delta": -1,
            },
            id="modifier-requires-one-optional-payment",
        ),
        pytest.param(
            "A.act.move.ref",
            {"ref": "this", "to": "deck", "position": "top"},
            {"ref": "this", "to": "hand", "position": "top"},
            id="position-requires-deck-destination",
        ),
        pytest.param(
            "A.instead.search",
            {"if": {"my_turn": True}, "n": 1},
            {"if": {"my_turn": True}, "n": 1, "upto": 1},
            id="replacement-quantity-is-exclusive",
        ),
        pytest.param(
            "A.target.exact",
            {
                "bind": "marked",
                "from": {"who": "opp", "zone": "field", "kind": "follower"},
                "n": 1,
                "distinct_names": True,
            },
            {
                "bind": "marked",
                "from": {"who": "opp", "zone": "field", "kind": "follower"},
                "n": 1,
                "distinct_names": True,
                "distinct_by": "name",
            },
            id="distinct-target-filters-are-exclusive",
        ),
    ],
)
def test_schema_rejects_conflicting_constraints(
    definition: str, control: JsonValue, invalid: JsonValue
) -> None:
    _assert_acceptance(control, definition=definition, accepted=True)
    _assert_acceptance(invalid, definition=definition, accepted=False)


def _local_author_references(node: JsonValue) -> JsonValue:
    if isinstance(node, list):
        return [_local_author_references(value) for value in node]
    if not isinstance(node, dict):
        return node
    return {
        key: value.removeprefix("urn:sve-kit:author-nodes:1.0")
        if key == "$ref" and isinstance(value, str)
        else _local_author_references(value)
        for key, value in node.items()
    }


def test_storage_ability_heads_follow_the_authority() -> None:
    storage = object_value(parse((SCHEMAS / "author-storage.schema.json").read_bytes()))
    cards = object_value(object_value(storage["properties"])["cards"])
    card = object_value(cards["additionalProperties"])
    abilities = object_value(object_value(card["properties"])["abilities"])
    variants = array(object_value(abilities["items"])["oneOf"])
    stored = {}
    for value in variants:
        variant = object_value(value)
        heads = {string(name) for name in array(variant["required"])} - {"line"}
        assert len(heads) == 1
        (head,) = heads
        assert head not in stored
        assert variant["additionalProperties"] is False
        stored[head] = object_value(variant["properties"])
    definitions = object_value(NODES["$defs"])
    assert stored.keys() == object_value(
        object_value(definitions["A.ability-head"])["properties"]
    ).keys() | {"use"}
    for reference in array(object_value(definitions["A.inline-ability"])["anyOf"]):
        name = string(object_value(reference)["$ref"]).removeprefix("#/$defs/")
        variant = object_value(definitions[name])
        heads = {string(key) for key in array(variant["required"])} - {
            "line",
            "sources",
            "ports",
        }
        assert len(heads) == 1
        (head,) = heads
        expected = {
            key: value
            for key, value in object_value(variant["properties"]).items()
            if key not in {"line", "section", "sources"}
        }
        actual = {
            key: value
            for key, value in stored[head].items()
            if key not in {"line", "face", "section", "scope", "admission", "covers"}
        }
        assert _local_author_references(actual) == _local_author_references(expected)
