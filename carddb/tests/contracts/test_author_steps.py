"""Public step decoding, recursive boundaries, and independent schema mutations."""

from copy import deepcopy
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import ValidationError

from sve_carddb.core.json import canonical, object_value

from .author_numeric_cases import NUMERIC
from .author_step_cases import CARD_STEPS, SYNTHETIC_ACTIONS, SYNTHETIC_CONTROLS
from .author_step_types import STEPS
from .test_author_schema_drift import NODES, assert_selector_definition

if TYPE_CHECKING:
    from pydantic import JsonValue


def _check(value: JsonValue, *, accepted: bool = True, name: str = "A.steps") -> None:
    validator = Draft202012Validator(
        {"$id": NODES["$id"], "$defs": NODES["$defs"], "$ref": "#/$defs/" + name}
    )
    assert validator.is_valid(value) == accepted
    adapter = (STEPS | NUMERIC)[name]
    if accepted:
        decoded = adapter.validate_json(canonical(value), strict=True)
        assert canonical(adapter.dump_python(decoded, mode="json")) == canonical(value)
    else:
        with pytest.raises(ValidationError):
            adapter.validate_json(canonical(value), strict=True)


@pytest.mark.parametrize(
    "program",
    [steps for _, _, steps in CARD_STEPS],
    ids=[card + "/" + position for card, position, _ in CARD_STEPS],
)
def test_card_programs_preserve_order_and_authored_fields(
    program: list[JsonValue],
) -> None:
    _check(program)


@pytest.mark.parametrize("action", SYNTHETIC_ACTIONS)
def test_synthetic_action_variants(action: dict[str, JsonValue]) -> None:
    _check([action])
    _check(action, name="A.action")
    _check(action, name="A.step")


@pytest.mark.parametrize("control", SYNTHETIC_CONTROLS)
def test_control_variants_are_not_actions(control: dict[str, JsonValue]) -> None:
    _check([control])
    _check(control, name="A.action", accepted=False)


@pytest.mark.parametrize("step", SYNTHETIC_ACTIONS + SYNTHETIC_CONTROLS)
def test_steps_are_closed_and_not_numeric_or_condition_nodes(
    step: dict[str, JsonValue],
) -> None:
    key, payload = next(iter(step.items()))
    _check([{**step, "unknown": True}], accepted=False)
    _check([{key: None}], accepted=False)
    if isinstance(payload, dict):
        _check([{key: {**payload, "unknown": True}}], accepted=False)
    _check(step, name="A.expr.numeric", accepted=False)
    _check(step, name="A.cond", accepted=False)
    assert not Draft202012Validator(NODES).is_valid(step)


@pytest.mark.parametrize(
    "payload",
    [
        {},
        {"draw": {"n": 1}, "life": {"who": "self", "delta": 1}},
        {"draw": {"n": True}},
        {"draw": {"n": -1}},
        {
            "search": {
                "sel": {"who": "self", "zone": "deck"},
                "n": 1,
                "to": "hand",
                "bind": "found_cards",
            }
        },
        {
            "search": {
                "sel": {"who": "self", "zone": "deck"},
                "n": 1,
                "to": {"per_group": ["hand", "field"]},
            }
        },
        {
            "search": {
                "groups": [{"sel": {"who": "self", "zone": "deck"}, "n": 1}],
                "to": "hand",
            }
        },
        {
            "search": {
                "groups": [
                    {"sel": {"who": "self", "zone": "deck"}, "n": 1},
                    {"sel": {"who": "self", "zone": "deck"}, "n": 1},
                ],
                "n": 1,
                "to": "hand",
            }
        },
        {"move": {"ref": "t", "top": {"who": "self", "n": 1}, "to": "hand"}},
        {"move": {"ref": "t", "to": "hand", "position": "top"}},
        {"move": {"ref": "t", "to": "deck", "order": "chooser"}},
        {"move": {"ref": "t", "to": "deck", "position": "top", "order": True}},
        {"move": {"ref": "t", "to": "ex", "acted": False}},
        {"move": {"ref": "t", "to": "field", "bind": "moved"}},
        {"move": {"ref": "t", "to": "deck", "position": {"nth": 0}}},
        {
            "move": {
                "ref": "t",
                "to": "deck",
                "position": {"one_of": ["top"], "by": "self"},
            }
        },
        {"move": {"bottom": {"who": "self", "n": 1}, "to": "hand"}},
        {"banish": {"sel": {"who": "self", "zone": "cemetery"}, "n": 1}},
        {"banish": {"bottom": {"who": "self", "n": 1}}},
        {"look": {"top": {"who": "self", "n": 1}, "bind": "seen", "random": {"n": 1}}},
        {"reveal": {"ref": "t", "bind": "shown", "random": True}},
        {"damage": {"to": "t", "n": 2, "distribute_at": "play"}},
        {"damage": {"to": "t", "n": 2, "distribute": False}},
        {"buff": {"on": "this"}},
        {"set_stat": {"on": "t", "power": -1}},
        {"grant": {"on": "this", "abilities": []}},
        {"declare": {"kind": "name", "bind": "declared", "max": 10}},
        {"box": {"ref": "t"}},
        {"pilot": {"ref": "this", "on": "t"}},
        {"play_card": {"ref": "t", "cost": 0}},
        {"feed": {"ref": "this"}},
        {"drive_check": {"n": 1}},
        {
            "play_ability": {
                "of": "this",
                "trigger": "fanfare",
                "ability": {"ub": True, "n": 10},
            }
        },
        {"create": {"token": "token", "to": "field", "n": 1, "upto": 2}},
        {
            "create": {
                "token": {"allocate": {"among": ["token"], "total": 1}},
                "to": "field",
                "n": 1,
            }
        },
        {"create": {"token": "token", "to": "ex", "acted": True}},
        {"create": {"token": "token", "to": "field", "counters": {}}},
        {"transform": {"ref": "t", "into": ["first", "second"]}},
        {"optional": {"do": []}},
        {"simultaneous": {"do": []}},
        {"choice": {"n": 1, "upto": 2, "options": [[]]}},
        {"choice": {"n": 1, "options": [], "instead": []}},
        {
            "choice": {
                "n": 1,
                "options": [[]],
                "instead": [
                    {"if": {"payment_decided": "optional"}, "upto": 2, "after": []}
                ],
            }
        },
        {"repeat": {"n": 1, "while": {"awakened": True}, "do": []}},
        {"repeat": {"n": 1, "check": "before", "do": []}},
        {"either": {"options": [[]]}},
        {
            "each_player": {
                "stages": [{"do": [], "mode": "choice", "evidence": "unused"}]
            }
        },
        {"each_player": {"stages": [{"do": [], "mode": "explicit"}]}},
        {"order": {"ref": "t"}},
        {"delayed": {"do": []}},
    ],
)
def test_wrong_type_exclusivity_and_pending_shapes(payload: JsonValue) -> None:
    _check([payload], accepted=False)


@pytest.mark.parametrize(
    "entry",
    [
        {"if": {"awakened": True}},
        {"if": {"awakened": True}, "n": 2, "upto": 3},
        {"if": {"awakened": True}, "n": 2, "replace_with": {"draw": {"n": 2}}},
        {
            "if": {"awakened": True},
            "replace_with": {"if": {"cond": {"awakened": True}, "then": []}},
        },
        {"if": {"awakened": True}, "result": "rebound"},
    ],
)
def test_action_replacement_does_not_accept_controls_or_rebind_results(
    entry: JsonValue,
) -> None:
    _check([{"draw": {"n": 1, "instead": [entry]}}], accepted=False)


@pytest.mark.parametrize(
    ("name", "field"),
    [
        ("A.instead.damage", "n"),
        ("A.instead.draw", "n"),
        ("A.instead.move", "ref"),
        ("A.instead.banish", "ref"),
        ("A.instead.destroy", "ref"),
        ("A.instead.buff", "power"),
        ("A.instead.life", "delta"),
        ("A.instead.grant", "keywords"),
        ("A.instead.create", "upto"),
        ("A.instead.search", "upto"),
    ],
)
def test_each_replacement_vocabulary(name: str, field: str) -> None:
    value: JsonValue = "t" if field == "ref" else ["疾走"] if field == "keywords" else 2
    _check({"if": {"awakened": True}, field: value}, name=name)
    _check({"if": {"awakened": True}, "unknown": value}, name=name, accepted=False)


def test_recursive_program_preserves_empty_blocks_duplicates_and_order() -> None:
    _check([])
    program: JsonValue = [{"draw": {"n": 1}}, {"draw": {"n": 1}}]
    for _ in range(8):
        program = [{"if": {"cond": {"awakened": True}, "then": program, "else": []}}]
    _check(program)


@pytest.mark.parametrize(
    "value", ["additional", True, {"of": "play", "kind": "optional"}, None]
)
def test_payment_decision_is_not_paid_or_a_numeric_receipt(value: JsonValue) -> None:
    _check({"payment_decided": value}, name="A.cond", accepted=False)
    _check({"payment_decided": "optional"}, name="A.expr.numeric", accepted=False)


def test_payment_decision_has_a_separate_preserved_wire_key() -> None:
    _check({"payment_decided": "optional"}, name="A.cond")
    _check(
        {"all": [{"payment_decided": "optional"}, {"not": {"paid": "optional"}}]},
        name="A.cond",
    )


@pytest.mark.parametrize(
    ("name", "mutation"),
    [
        ("A.act.draw", "required"),
        ("A.act.box", "required"),
        ("A.act.move.ref", "extra"),
        ("A.position.nth", "type"),
        ("A.control.repeat.count", "reference"),
        ("A.control.choice.exact", "minimum"),
        ("A.move-order", "enum"),
        ("A.act.search.grouped", "reference"),
        ("A.step", "maximum"),
        ("A.steps", "reference"),
    ],
)
def test_step_schema_drift_detects_mutations(name: str, mutation: str) -> None:
    schema = deepcopy(NODES)
    node = object_value(object_value(schema["$defs"])[name])
    if mutation == "required":
        node.pop("required")
    elif mutation == "extra":
        node["additionalProperties"] = True
    elif mutation == "type":
        object_value(object_value(node["properties"])["nth"])["type"] = "string"
    elif mutation == "minimum":
        object_value(object_value(node["properties"])["options"])["minItems"] = 0
    elif mutation == "enum":
        node["enum"] = ["chooser", "shuffle", "random"]
    elif mutation == "maximum":
        node["maxProperties"] = 2
    elif name == "A.steps":
        node["items"] = {"$ref": "#/$defs/A.action"}
    else:
        field = "do" if name == "A.control.repeat.count" else "to"
        object_value(node["properties"])[field] = {"$ref": "#/$defs/A.effect"}
    with pytest.raises(AssertionError):
        assert_selector_definition(schema, name)


@pytest.mark.parametrize(
    "step",
    [
        {
            "damage": {
                "to": "t",
                "n": 1,
                "instead": [{"if": {"awakened": True}, "to": {"param": "target"}}],
            }
        },
        {"equip": {"on": "this", "token": {"param": "token"}}},
        {"evolve": {"ref": "this", "into": {"param": "name"}}},
        {
            "search": {
                "groups": [
                    {"sel": {"who": "self", "zone": "deck"}, "n": {"param": "count"}},
                    {"sel": {"who": "self", "zone": "deck"}, "n": 1},
                ],
                "to": "hand",
            }
        },
        {
            "search": {
                "sel": {"who": "self", "zone": "deck"},
                "n": 1,
                "to": {
                    "zone": "hand",
                    "instead": {
                        "if": {"awakened": True},
                        "zone": {"param": "destination"},
                    },
                },
            }
        },
        {
            "create": {
                "token": {
                    "allocate": {"among": ["token"], "total": {"param": "count"}}
                },
                "to": "field",
            }
        },
        {"move": {"ref": {"first": "seen"}, "to": "hand"}},
        {"move": {"ref": {"rest": "seen"}, "to": "hand"}},
        {"move": {"ref": {"top": {"who": "self", "n": 1}}, "to": "hand"}},
    ],
)
def test_parameter_roles_and_undefined_reference_aliases(step: JsonValue) -> None:
    _check([step], accepted=False)
