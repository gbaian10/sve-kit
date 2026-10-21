"""Validate recursive author programs against independent Python and JSON decoders."""

import json
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import ValidationError

from sve_carddb.core.json import canonical

from .author_ability_cases import CARD_ABILITIES
from .author_ability_types import ABILITIES
from .author_cost_cases import COSTS
from .author_effect_cases import EFFECTS
from .author_step_types import STEPS
from .test_author_schema_drift import NODES, REFERENCES

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
