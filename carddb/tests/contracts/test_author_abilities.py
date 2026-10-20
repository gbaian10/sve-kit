"""Validate recursive author programs against independent Python and JSON decoders."""

import json
from copy import deepcopy
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import ValidationError

from sve_carddb.contracts.author_programs import Action, GrantedAbilityHead, Step
from sve_carddb.core.json import canonical, object_value

from .author_ability_cases import CARD_ABILITIES
from .author_ability_types import ABILITIES
from .author_cost_cases import COSTS
from .author_effect_cases import EFFECTS
from .author_step_types import STEPS
from .test_author_schema_drift import NODES, REFERENCES, assert_selector_definition

if TYPE_CHECKING:
    from pydantic import JsonValue

EVENT: dict[str, JsonValue] = {"phase": {"which": "end", "whose": "self"}}
COND: dict[str, JsonValue] = {"acted": "this"}
TRIGGERS = (
    "fanfare",
    "lastword",
    "evolve_time",
    "super_evolve_time",
    "attack_time",
    "race_time",
    "drive_time",
)
HEADS: dict[str, JsonValue] = {
    **{key: {"do": []} for key in TRIGGERS},
    "keywords": ["ward"],
    "quick": True,
    "auto": {"on": EVENT, "do": []},
    "activate": {"cost": {"pp": 1}, "do": []},
    "spell": {"do": []},
    "evolve": {"cost": {"pp": 1}},
    "ride": {"cost": {"pp": 1}},
    "feed": {"cost": {"pp": 1}},
    "static": {"effects": [{"keywords": {"on": "this", "list": ["ward"]}}]},
    "play": {"cost_delta": -1},
}


def check(
    value: JsonValue, *, name: str = "A.ability-head", valid: bool = True
) -> None:
    schema = {"$defs": NODES["$defs"], "$ref": "#/$defs/" + name}
    assert Draft202012Validator(schema).is_valid(value) == valid
    adapter = (ABILITIES | COSTS | STEPS | EFFECTS | REFERENCES)[name]
    if valid:
        decoded = adapter.validate_json(json.dumps(value), strict=True)
        assert canonical(adapter.dump_python(decoded, mode="json")) == canonical(value)
    else:
        with pytest.raises(ValidationError):
            adapter.validate_json(json.dumps(value), strict=True)


@pytest.mark.parametrize(
    "head",
    [head for _, _, head in CARD_ABILITIES],
    ids=[card + "/" + label for card, label, _ in CARD_ABILITIES],
)
def test_frozen_card_fragments(head: dict[str, JsonValue]) -> None:
    check(head)


@pytest.mark.parametrize(("key", "payload"), HEADS.items())
def test_every_head_and_grant_subset(key: str, payload: JsonValue) -> None:
    head = {key: payload}
    check(head)
    check(
        head,
        name="A.granted-ability-head",
        valid=key
        not in {
            "quick",
            "keywords",
            "spell",
            "evolve",
            "ride",
            "feed",
        },
    )
    check({**head, "unknown": True}, valid=False)
    check({key: None}, valid=False)
    check({**head, "line": 1}, valid=False)
    if isinstance(payload, dict):
        check({key: {**payload, "unknown": True}}, valid=False)
    assert not Draft202012Validator(NODES).is_valid(head)
    check(head, name="A.action", valid=False)


@pytest.mark.parametrize("key", [*TRIGGERS, "auto", "activate"])
@pytest.mark.parametrize(
    "n", [1, 2, 4, 9007199254740991, True, False, 0, -1, "1", None, 9007199254740992]
)
def test_limits_are_positive_safe_integers(key: str, n: JsonValue) -> None:
    payload = object_value(HEADS[key])
    check(
        {key: {**payload, "per_turn": n}},
        valid=type(n) is int and 1 <= n <= 9007199254740991,
    )
    check({key: {**payload, "once_per_turn": True}}, valid=False)


@pytest.mark.parametrize("key", TRIGGERS)
def test_trigger_wrappers_and_zone_boundary(key: str) -> None:
    payload: dict[str, JsonValue] = {
        "do": [{"draw": {"n": {"param": "amount"}}}],
        "targets": [{"bind": "t", "from": {"who": "opp", "zone": "field"}, "upto": 1}],
        "optional_cost": {"pp": "X", "result": "paid"},
        "x": {},
        "define": {"amount": 2},
        "ub": True,
    }
    check({key: payload})
    check({key: {**payload, "cost": {"pp": 1}}}, valid=key != "lastword")
    check({key: {**payload, "zone": "field"}}, valid=False)
    check({key: {**payload, "targets": []}}, valid=False)
    check({key: {**payload, "define": {"this": 1}}}, valid=False)
    check({key: {**payload, "define": {}}}, valid=False)
    check({key: {**payload, "ub": False}}, valid=False)


@pytest.mark.parametrize("key", ["auto", "activate", "static"])
@pytest.mark.parametrize(
    "zone", ["equipment", "hand", ["ex", "ex"], [], "banished", {"param": "zone"}]
)
def test_ability_zone_is_distinct_from_selector_zone(key: str, zone: JsonValue) -> None:
    check(
        {key: {**object_value(HEADS[key]), "zone": zone}},
        valid=zone in ("equipment", "hand", ["ex", "ex"]),
    )


@pytest.mark.parametrize(("tag", "amount"), [("n", 1), ("upto", 2), ("any", True)])
def test_fusion_is_one_material_selection(tag: str, amount: JsonValue) -> None:
    fusion: dict[str, JsonValue] = {
        "sel": {"who": "self"},
        tag: amount,
        "result": "fused",
    }
    head: dict[str, JsonValue] = {
        "zone": "hand",
        "fusion": fusion,
        "x": {"from_payment": "fused"},
        "do": [],
    }
    check({"activate": head})
    check({"activate": {**head, "cost": {"pp": 1}}}, valid=False)
    check({"activate": {**head, "quick": True}}, valid=False)
    check({"activate": {**head, "advance": True}}, valid=False)
    other = "upto" if tag == "n" else "n"
    check({"activate": {**head, "fusion": {**fusion, other: 1}}}, valid=False)


@pytest.mark.parametrize(
    "into",
    [
        "深緑の純心・ポーラ",
        ["深緑の純心・ポーラ", "真紅の絆・ポーラ"],
        {"who": "self", "zone": "evolve_deck"},
        [],
        {"sel": {"who": "self", "zone": "evolve_deck"}},
        {"param": "name"},
    ],
)
def test_evolution_destination_is_shared(into: JsonValue) -> None:
    valid = into not in (
        [],
        {"sel": {"who": "self", "zone": "evolve_deck"}},
        {"param": "name"},
    )
    check({"evolve": {"cost": {"pp": 1}, "into": into}}, valid=valid)
    check({"evolve": {"ref": "this", "into": into}}, name="A.action", valid=valid)


@pytest.mark.parametrize(
    "key", ["cost_delta", "cost_set", "optional_cost_delta", "optional_cost_set"]
)
def test_conditional_modifiers_preserve_order(key: str) -> None:
    field = "delta" if key.endswith("delta") else "set"
    one: dict[str, JsonValue] = {"if": COND, field: 2}
    base: dict[str, JsonValue] = (
        {"optional_cost": {"pp": 1}} if key.startswith("optional_") else {}
    )
    valid_values: tuple[JsonValue, ...] = (2, one, [one, 3, one])
    for value in valid_values:
        check({"play": {**base, key: value}})
        check({**base, key: value}, name="A.play.costs")
    invalid_values: tuple[JsonValue, ...] = (
        [],
        [one],
        {"if": COND},
        {"if": COND, field: True},
        {"param": "cost"},
    )
    for value in invalid_values:
        check({"play": {**base, key: value}}, valid=False)
    if base:
        check({"play": {key: one}}, valid=False)
        check(
            {"play": {"optional_cost": [{"pp": 1}, {"pp": 2}], key: one}}, valid=False
        )


@pytest.mark.parametrize("zone", ["cemetery", ["cemetery", "ex"]])
def test_conditional_zone_permission(zone: JsonValue) -> None:
    check({"play": {"playable_from": {"zone": zone, "if": COND}}})
    check({"play": {"playable_from": zone}})
    check({"play": {"playable_from": {"zone": zone}}}, valid=False)


@pytest.mark.parametrize(
    "head",
    [
        {},
        {"quick": 1},
        {"quick": False},
        {"keywords": []},
        {"fanfare": {}},
        {"auto": {"do": []}},
        {"activate": {"do": []}},
        {"static": {"effects": []}},
        {"static": {"effects": [{"draw": {"n": 1}}]}},
        {"play": {}},
        {"play": {"define": {"n": 1}}},
        {"play": {"playable_from": []}},
        {"play": {"playable_from": {"zone": [], "if": COND}}},
        {"spell": {"do": [], "cost": {"pp": 1}}},
        {"spell": {"do": [], "per_turn": 1}},
        {"feed": {"cost": {"pp": 1}, "times": 0}},
        {"feed": {"cost": {"pp": 1}, "times": True}},
        {"ride": {"cost": {"pp": 1}, "times": 2}},
    ],
)
def test_invalid_heads(head: JsonValue) -> None:
    check(head, valid=False)


@pytest.mark.parametrize(
    "context", ["A.act.grant", "A.effect.grant", "A.instead.grant"]
)
def test_grant_content_and_metadata_boundaries(context: str) -> None:
    base: dict[str, JsonValue] = (
        {"on": {"leader": "self"}}
        if context == "A.act.grant"
        else {"if": COND}
        if context == "A.instead.grant"
        else {}
    )
    valid_contents: tuple[dict[str, JsonValue], ...] = (
        {"keywords": ["quick"]},
        {"abilities": [{"lastword": {"do": []}}]},
        {"keywords": ["storm"], "abilities": [{"attack_time": {"do": []}}]},
    )
    for content in valid_contents:
        check({**base, **content}, name=context)
    invalid_contents: tuple[dict[str, JsonValue], ...] = (
        {},
        {"keywords": []},
        {"abilities": []},
        {"abilities": [{"quick": True}]},
        {"abilities": [{"spell": {"do": []}}]},
        {"abilities": [{"lastword": {"do": [], "line": 1}}]},
    )
    for content in invalid_contents:
        check({**base, **content}, name=context, valid=False)
    check(
        {**base, "keywords": [{"param": "kw"}]},
        name=context,
        valid=context != "A.effect.grant",
    )
    if context == "A.instead.grant":
        check(
            {
                **base,
                "abilities": [{"lastword": {"do": []}}],
                "replace_with": {"draw": {"n": 1}},
            },
            name=context,
            valid=False,
        )


@pytest.mark.parametrize("until", [None, "turn", {"during_next_turn": {"who": "opp"}}])
@pytest.mark.parametrize("next_only", [None, True, False])
def test_delayed_lifetime_contract(until: JsonValue, next_only: JsonValue) -> None:
    payload: dict[str, JsonValue] = {"on": EVENT, "do": []}
    if until is not None:
        payload["until"] = until
    if next_only is not None:
        payload["next_only"] = next_only
    valid = not (until is None and next_only is False)
    check({"delayed": payload}, name="A.action", valid=valid)
    check({"delayed": payload}, name="A.step", valid=valid)


@pytest.mark.parametrize(
    ("key", "value"),
    [
        ("line", 1),
        ("result", "installed"),
        ("cost", {"pp": 1}),
        ("zone", "field"),
        ("per_turn", 1),
        ("ub", True),
        ("when", EVENT),
        ("next_only", 1),
        ("aggregation", "each"),
    ],
)
def test_delayed_rejects_unowned_fields(key: str, value: JsonValue) -> None:
    check({"on": EVENT, "do": [], key: value}, name="A.act.delayed", valid=False)


def test_deep_recursion_and_parameter_positions() -> None:
    delayed: dict[str, JsonValue] = {
        "on": EVENT,
        "precondition": COND,
        "aggregation": "batch",
        "do": [
            {
                "grant": {
                    "on": "this",
                    "abilities": [
                        {"lastword": {"do": [{"draw": {"n": {"param": "draw"}}}]}}
                    ],
                }
            }
        ],
    }
    head: dict[str, JsonValue] = {
        "static": {
            "zone": "equipment",
            "effects": [
                {
                    "grant": {
                        "on": "host",
                        "abilities": [
                            {"auto": {"on": EVENT, "do": [{"delayed": delayed}]}}
                        ],
                    }
                }
            ],
        }
    }
    check(head)
    check(
        {
            "on": "this",
            "keywords": ["ward"],
            "instead": [{"if": COND, "replace_with": {"delayed": delayed}}],
        },
        name="A.act.grant",
    )
    body = delayed["do"]
    assert isinstance(body, list)
    object_value(body[0])["grant"] = {"on": "this", "abilities": [{"param": "ability"}]}
    check(head, valid=False)


@pytest.mark.parametrize(
    "value",
    [
        {"during_next_turn": {"who": "opp"}},
        {"during_next_turn": {"who": {"master_of": "t"}}},
        {"during_next_turn": {}},
        {"during_next_turn": {"who": "opp", "edge": "end"}},
        {"during_next_turn": {"who": {"param": "who"}}},
    ],
)
def test_full_next_turn_interval(value: JsonValue) -> None:
    check(
        value,
        name="A.until",
        valid=value
        in (
            {"during_next_turn": {"who": "opp"}},
            {"during_next_turn": {"who": {"master_of": "t"}}},
        ),
    )


@pytest.mark.parametrize(
    "mutation", ["required", "ref", "enum", "union", "minItems", "extra", "exclusive"]
)
def test_ability_drift_mutations(mutation: str) -> None:
    changed = deepcopy(NODES)
    defs = object_value(changed["$defs"])
    name = "A.head.trigger"
    node = object_value(defs[name])
    props = object_value(node["properties"])
    if mutation == "required":
        node["required"] = []
    elif mutation == "ref":
        props["do"] = {"$ref": "#/$defs/A.action"}
    elif mutation == "enum":
        name = "A.ability-zone"
        object_value(defs[name])["anyOf"] = [{"const": "equipment"}]
    elif mutation == "union":
        name = "A.head.activate"
        object_value(defs[name])["anyOf"] = [{"$ref": "#/$defs/A.head.activate.cost"}]
    elif mutation == "minItems":
        name = "A.head.static"
        object_value(object_value(object_value(defs[name])["properties"])["effects"])[
            "minItems"
        ] = 0
    elif mutation == "extra":
        node["additionalProperties"] = True
    else:
        name = "A.granted-ability-head"
        object_value(defs[name])["maxProperties"] = 2
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


def assert_action_edges(schema: dict[str, JsonValue]) -> None:
    defs = object_value(schema["$defs"])
    action = object_value(object_value(defs["A.action"])["properties"])
    step = object_value(object_value(defs["A.step"])["properties"])
    assert all(step[key] == value for key, value in action.items())
    for context in ("A.action", "A.step"):
        assert object_value(object_value(defs[context])["properties"])["delayed"] == {
            "$ref": "#/$defs/A.act.delayed"
        }
    for key, value in object_value(
        object_value(defs["A.granted-ability-head"])["properties"]
    ).items():
        assert (
            object_value(object_value(defs["A.ability-head"])["properties"])[key]
            == value
        )


def test_shared_payload_edges_and_mutation() -> None:
    assert_action_edges(NODES)
    changed = deepcopy(NODES)
    defs = object_value(changed["$defs"])
    object_value(object_value(defs["A.step"])["properties"])["delayed"] = {
        "$ref": "#/$defs/A.head.auto"
    }
    with pytest.raises(AssertionError):
        assert_action_edges(changed)


def test_delayed_accepts_owned_wrappers() -> None:
    check(
        {
            "on": EVENT,
            "targets": [
                {"bind": "t", "from": {"who": "opp", "zone": "field"}, "upto": 1}
            ],
            "optional_cost": {"pp": "X", "result": "paid"},
            "x": {},
            "define": {"amount": 2},
            "do": [{"damage": {"to": "t", "n": 2}}],
        },
        name="A.act.delayed",
    )


def test_nominal_roots_expose_closed_fields_without_wire_wrapper() -> None:
    action = Action.model_validate_json('{"draw":{"n":1}}')
    step = Step.model_validate_json('{"draw":{"n":1}}')
    ability = GrantedAbilityHead.model_validate_json(
        '{"lastword":{"do":[{"draw":{"n":1}}]}}'
    )
    assert action.root["draw"]["n"] == 1
    assert step.root["draw"]["n"] == 1
    assert ability.root["lastword"]["do"][0].root["draw"]["n"] == 1
    assert json.loads(action.model_dump_json()) == {"draw": {"n": 1}}
    assert json.loads(step.model_dump_json()) == {"draw": {"n": 1}}
    assert json.loads(ability.model_dump_json()) == {
        "lastword": {"do": [{"draw": {"n": 1}}]}
    }
