"""Effect installation boundaries, distinct from actions and parent admission."""

from copy import deepcopy
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import ValidationError

from sve_carddb.core.json import canonical, object_value

from .author_effect_cases import CARD_EFFECTS, EFFECTS, SYNTHETIC_EFFECTS
from .author_history_cases import HISTORY
from .author_numeric_cases import NUMERIC
from .test_author_schema_drift import NODES, assert_selector_definition

if TYPE_CHECKING:
    from pydantic import JsonValue


def _values(*values: JsonValue) -> tuple[JsonValue, ...]:
    return values


def _check(value: JsonValue, *, accepted: bool = True, name: str = "A.effect") -> None:
    validator = Draft202012Validator(
        {"$defs": NODES["$defs"], "$ref": "#/$defs/" + name}
    )
    assert validator.is_valid(value) == accepted
    adapter = EFFECTS[name]
    if accepted:
        decoded = adapter.validate_json(canonical(value), strict=True)
        assert canonical(adapter.dump_python(decoded, mode="json")) == canonical(value)
    else:
        with pytest.raises(ValidationError):
            adapter.validate_json(canonical(value), strict=True)


@pytest.mark.parametrize(
    "effect",
    [effect for _, _, effect in CARD_EFFECTS],
    ids=[card + "/" + position for card, position, _ in CARD_EFFECTS],
)
def test_real_card_effect_fragments(effect: dict[str, JsonValue]) -> None:
    _check(effect)


@pytest.mark.parametrize("effect", SYNTHETIC_EFFECTS)
def test_synthetic_effect_fragments(effect: dict[str, JsonValue]) -> None:
    _check(effect)


@pytest.mark.parametrize(
    "effect", [effect for _, _, effect in CARD_EFFECTS] + SYNTHETIC_EFFECTS
)
def test_effects_are_closed_and_do_not_enter_other_expression_contexts(
    effect: dict[str, JsonValue],
) -> None:
    key, payload = next(iter(effect.items()))
    _check({key: {**object_value(payload), "unknown": True}}, accepted=False)
    _check({**effect, "unknown": {}}, accepted=False)
    _check({key: None}, accepted=False)
    for name, adapter in {
        "A.event": HISTORY["A.event"],
        "A.expr.numeric": NUMERIC["A.expr.numeric"],
    }.items():
        assert not Draft202012Validator(
            {"$defs": NODES["$defs"], "$ref": "#/$defs/" + name}
        ).is_valid(effect)
        with pytest.raises(ValidationError):
            adapter.validate_json(canonical(effect), strict=True)
    assert not Draft202012Validator(NODES).is_valid(effect)


@pytest.mark.parametrize(
    "effect",
    [
        {},
        {"stat": {"power": 1}, "keywords": {"list": ["storm"]}},
        {"grant": {}},
        {"grant": {"keywords": []}},
        {"buff": {"power": 1}},
        {"apply": {"on": "this", "effect": {"stat": {"power": 1}}}},
        {"static": {"effect": {"stat": {"power": 1}}}},
    ],
)
def test_effect_root_rejects_actions_headers_and_pending_grant(
    effect: JsonValue,
) -> None:
    _check(effect, accepted=False)


@pytest.mark.parametrize("key", ["stat", "set_stat"])
def test_stat_requires_at_least_one_dimension(key: str) -> None:
    for value in _values({"power": 0}, {"hp": 0}, {"power": 1, "hp": 2}):
        _check({key: value})
    _check({key: {}}, accepted=False)
    _check({key: {"on": "this"}}, accepted=False)
    _check({key: {"hp": True}}, accepted=False)
    _check({key: {"hp": {"param": "n"}}}, accepted=False)
    _check({key: {"hp": {"plus": [1, {"param": "n"}]}}})
    _check({key: {"hp": -1}}, accepted=key == "stat")


@pytest.mark.parametrize(
    ("key", "base", "left", "right"),
    [
        ("cost_mod", {"when_playing": {"in": "t"}}, "delta", "set"),
        ("damage_mod", {"on": "this"}, "delta", "multiply"),
        ("evolve_cost_mod", {"on": "this"}, "delta", "set"),
    ],
)
def test_adjustments_are_exclusive_and_signed_only_for_delta(
    key: str, base: dict[str, JsonValue], left: str, right: str
) -> None:
    _check({key: {**base, left: -1}})
    _check({key: {**base, right: 0}})
    _check({key: base}, accepted=False)
    _check({key: {**base, left: 1, right: 1}}, accepted=False)
    _check({key: {**base, right: -1}}, accepted=False)
    for field in [left, right]:
        _check({key: {**base, field: True}}, accepted=False)
        _check({key: {**base, field: {"param": "n"}}}, accepted=key == "cost_mod")


@pytest.mark.parametrize(
    "kind", ["any", "attack", "combat", "ability", "effect", "all"]
)
def test_damage_kind_enums_are_not_interchangeable(kind: str) -> None:
    _check(
        {"damage_mod": {"kind": kind, "delta": 1}},
        accepted=kind in {"any", "attack", "combat", "ability"},
    )
    _check(
        {"prevent": {"kind": kind}},
        accepted=kind in {"any", "combat", "ability"},
    )


@pytest.mark.parametrize(
    ("key", "base"),
    [
        ("cost_mod", {"when_playing": {"in": "t"}, "delta": -1}),
        ("damage_mod", {"delta": 1}),
        ("prevent", {"kind": "any"}),
    ],
)
def test_next_only_is_strict_and_does_not_insert_defaults(
    key: str, base: dict[str, JsonValue]
) -> None:
    for value in [True, False]:
        _check({key: {**base, "next_only": value}})
    for wrong in _values(0, 1, "true", None):
        _check({key: {**base, "next_only": wrong}}, accepted=False)
    _check({key: base})


def test_damage_output_scope_remains_absent_for_parent_resolution() -> None:
    for source in ["this", "affected", "entering"]:
        _check({"damage_mod": {"source": source, "delta": 1}})
    _check({"damage_mod": {"source": {"leaders": "all"}, "delta": 1}}, accepted=False)
    _check({"damage_mod": {"on": {"leaders": "all"}, "delta": 1}})
    _check({"stat": {"on": {"leaders": "all"}, "power": 1}}, accepted=False)
    _check({"prevent": {"kind": "any", "source": {"leader": "self"}}}, accepted=False)
    _check({"unlimited_evolve": {"who": {"leader": "self"}}}, accepted=False)
    _check({"unlimited_evolve": {"who": "affected"}}, accepted=False)


@pytest.mark.parametrize("key", ["must_attack", "enters_acted", "ignore_ward"])
def test_required_object_scope_is_never_defaulted(key: str) -> None:
    _check({key: {"on": "affected"}})
    _check({key: {"on": "entering"}})
    _check({key: {}}, accepted=False)
    _check({key: {"who": "self"}}, accepted=False)


def test_no_trigger_uses_one_explicit_scope_and_trigger_kinds() -> None:
    for scope in [{"on": "entering"}, {"who": "opponents"}]:
        _check({"no_trigger": {**scope, "kinds": ["fanfare", "evolve_time"]}})
    _check({"no_trigger": {"kinds": ["fanfare"]}}, accepted=False)
    _check(
        {"no_trigger": {"on": "this", "who": "self", "kinds": ["fanfare"]}},
        accepted=False,
    )
    _check(
        {"no_trigger": {"on": {"kind": "follower"}, "kinds": ["fanfare"]}},
        accepted=False,
    )
    for kinds in _values([], ["activated"], ["evolution"], ["storm"], [{"param": "k"}]):
        _check({"no_trigger": {"on": "this", "kinds": kinds}}, accepted=False)


@pytest.mark.parametrize("what", ["play_card", "play_ability"])
def test_cannot_play_has_exclusive_roles_but_defers_parent_defaults(what: str) -> None:
    _check({"cannot": {"what": what}})
    _check({"cannot": {"what": what, "on": "this"}})
    _check({"cannot": {"what": what, "who": "self"}})
    _check({"cannot": {"what": what, "on": "this", "who": "self"}}, accepted=False)
    wrong_filter = "ability" if what == "play_card" else "card"
    _check(
        {"cannot": {"what": what, wrong_filter: {"kind": "follower"}}}, accepted=False
    )


@pytest.mark.parametrize(
    ("what", "wrong_field", "value"),
    [
        ("draw", "on", "this"),
        ("attack", "who", "opp"),
        ("choose_target", "by", {"ability_of": "this"}),
        ("destroy", "by", "opp"),
        ("banish", "by", "rule"),
        ("stand", "who", "self"),
        ("win", "on", "this"),
        ("lose", "during", {"phase": "start", "cause": "rule"}),
        ("pp_max_increase", "card", {"kind": "follower"}),
        ("deal_damage", "target", {"leader": "self"}),
    ],
)
def test_cannot_variants_do_not_share_other_variants_fields(
    what: str, wrong_field: str, value: JsonValue
) -> None:
    _check({"cannot": {"what": what, wrong_field: value}}, accepted=False)


def test_cannot_during_uses_separate_closed_shapes() -> None:
    draw: list[JsonValue] = [
        {"not_phase": "start"},
        {"phase": "start", "cause": "rule"},
    ]
    stand: JsonValue = {"phase": "start", "who": "self", "cause": "rule"}
    for during in draw:
        _check({"cannot": {"what": "draw", "during": during}})
        _check({"cannot": {"what": "stand", "during": during}}, accepted=False)
    _check({"cannot": {"what": "stand", "during": stand}})
    _check({"cannot": {"what": "draw", "during": stand}}, accepted=False)
    for during in _values(
        {},
        {"phase": "start"},
        {"not_phase": "start", "cause": "rule"},
        {"not_phase": "start", "phase": "end", "cause": "rule"},
        {"phase": "main", "who": "self", "cause": "rule"},
    ):
        for what in ["draw", "stand"]:
            _check({"cannot": {"what": what, "during": during}}, accepted=False)


@pytest.mark.parametrize(
    "predicate",
    [
        {},
        {"who": "self"},
        {"in": "t"},
        {"where": {"standing": "this"}},
        {"name": "a", "names": ["b"]},
        {"token": True, "not_token": True},
        {"kind": {"param": "kind"}},
        {"zone": []},
        {"zone": "banished"},
    ],
)
def test_card_predicate_has_filters_without_selection_authority(
    predicate: JsonValue,
) -> None:
    _check({"cannot": {"what": "play_card", "card": predicate}}, accepted=False)


def test_ability_predicate_and_lists_are_closed_and_preserve_multiplicity() -> None:
    for predicate in _values(
        {"kind": "activated", "except": "evolution"},
        {"kind": "activated", "except": []},
        {"kind": "activated", "except": ["fanfare"]},
        {"keyword": "fanfare", "kind": "activated"},
    ):
        _check(
            {"cannot": {"what": "play_ability", "ability": predicate}}, accepted=False
        )
    for words in _values(["storm", "ward"], ["ward", "storm"], ["storm", "storm"]):
        _check({"keywords": {"list": words}})
    for words in _values([], [{"param": "k"}]):
        _check({"keywords": {"list": words}}, accepted=False)
    _check({"keywords": {"list": ["守護"]}})
    _check(
        {
            "cannot": {
                "what": "play_ability",
                "ability": {"kind": "activated", "except": ["evolution", "evolution"]},
            }
        }
    )
    _check({"play_permission": {"on": "this", "from": ["cemetery", "cemetery"]}})


@pytest.mark.parametrize(
    "value",
    [
        {"rule": "counter_cap", "on": "this", "name": "stack", "max": -1},
        {"rule": "counter_cap", "on": "this", "max": 7},
        {"rule": "counter_cap", "who": "self", "name": "stack", "max": 7},
        {"rule": "battle_damage_basis", "on": "this", "basis": "power"},
        {"rule": "battle_damage_basis", "on": "this", "basis": "hp", "max": 7},
        {"rule": "spellchain_population", "who": "self", "of": {"leader": "self"}},
        {"rule": "unlimited_evolve", "who": "self"},
        {"rule": "extra_trigger", "phase": "end", "who": "self", "n": 1},
    ],
)
def test_rule_overrides_are_closed_variants(value: JsonValue) -> None:
    _check({"rule_override": value}, accepted=False)


def test_banish_replacement_does_not_extend_zones() -> None:
    _check({"banish_instead": {"from": "field", "to": "cemetery"}})
    for source, dest in [("hand", "cemetery"), ("field", "banish"), ("field", "ex")]:
        _check({"banish_instead": {"from": source, "to": dest}}, accepted=False)
    _check({"play_permission": {"on": "this", "from": []}}, accepted=False)
    _check({"play_permission": {"on": "this", "from": ["banish"]}}, accepted=False)


@pytest.mark.parametrize("name", EFFECTS)
def test_effect_drift_detects_constraint_removal(name: str) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])[name])
    if "properties" in node:
        node["additionalProperties"] = True
    else:
        node["anyOf"] = []
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize(
    ("name", "keyword"),
    [
        ("A.effect", "minProperties"),
        ("A.effect", "maxProperties"),
        ("A.effect.stat", "anyOf"),
        ("A.effect.set-stat", "anyOf"),
        ("A.effect.cost-mod", "oneOf"),
        ("A.effect.damage-mod", "oneOf"),
        ("A.effect.evolve-cost-mod", "oneOf"),
        ("A.effect.no-trigger", "oneOf"),
        ("A.effect.cannot.play-card", "not"),
        ("A.effect.card-predicate", "minProperties"),
        ("A.effect.cannot.draw-rule-phase", "required"),
        ("A.effect.cannot.stand-rule-phase", "required"),
    ],
)
def test_effect_drift_detects_cross_field_constraint_removal(
    name: str, keyword: str
) -> None:
    changed = deepcopy(NODES)
    object_value(object_value(changed["$defs"])[name]).pop(keyword)
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize(
    ("name", "field", "replacement"),
    [
        ("A.effect.prevent", "kind", {"enum": ["any", "attack", "combat", "ability"]}),
        ("A.effect.cost-mod", "set", {"$ref": "#/$defs/A.expr.numeric"}),
        ("A.effect.banish-instead", "from", {"$ref": "#/$defs/Zone"}),
        ("A.effect.cannot.attack", "target", {"$ref": "#/$defs/A.ref"}),
        ("A.effect.cannot.draw-rule-phase", "cause", {"type": "string"}),
        ("A.effect.keywords", "list", {"type": "array", "items": {"type": "string"}}),
    ],
)
def test_effect_drift_detects_field_type_widening(
    name: str, field: str, replacement: JsonValue
) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])[name])
    object_value(node["properties"])[field] = replacement
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)
