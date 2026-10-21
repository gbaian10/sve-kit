"""Effect installation boundaries, distinct from actions and parent admission."""

from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.core.json import canonical

from .author_effect_cases import CARD_EFFECTS, EFFECTS, SYNTHETIC_EFFECTS

if TYPE_CHECKING:
    from pydantic import JsonValue


def _values(*values: JsonValue) -> tuple[JsonValue, ...]:
    return values


def _check(value: JsonValue, *, accepted: bool = True, name: str = "A.effect") -> None:
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
    "effect",
    [{}, {"stat": {"power": 1}, "keywords": {"list": ["storm"]}}],
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
