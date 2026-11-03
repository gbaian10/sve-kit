"""Step decoding preserves card programs and rejects invalid recursive boundaries."""

from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.core.json import canonical

from .author_numeric_cases import NUMERIC
from .author_step_cases import CARD_STEPS, SYNTHETIC_ACTIONS, SYNTHETIC_CONTROLS
from .author_step_types import STEPS

if TYPE_CHECKING:
    from pydantic import JsonValue


def _check(value: JsonValue, *, accepted: bool = True, name: str = "A.steps") -> None:
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


@pytest.mark.parametrize(
    "payload",
    [{}, {"draw": {"n": 1}, "life": {"who": "self", "delta": 1}}],
)
def test_wrong_type_exclusivity_and_pending_shapes(payload: JsonValue) -> None:
    _check([payload], accepted=False)


def test_recursive_program_preserves_empty_blocks_duplicates_and_order() -> None:
    _check([])
    program: JsonValue = [{"draw": {"n": 1}}, {"draw": {"n": 1}}]
    for _ in range(8):
        program = [{"if": {"cond": {"awakened": True}, "then": program, "else": []}}]
    _check(program)


def test_payment_decision_has_a_separate_preserved_wire_key() -> None:
    _check({"payment_decided": "optional"}, name="A.cond")
    _check(
        {"all": [{"payment_decided": "optional"}, {"not": {"paid": "optional"}}]},
        name="A.cond",
    )


@pytest.mark.parametrize(
    "token",
    [
        {},
        {"same_name_as": ""},
        {"same_name_as": "self"},
        {"same_name_as": "it", "extra": True},
        {"same_name_as": "chosen", "kind": "amulet"},
    ],
)
def test_same_name_token_requires_a_closed_object_reference(token: JsonValue) -> None:
    _check({"create": {"token": token, "to": "field"}}, accepted=False, name="A.action")


@pytest.mark.parametrize(
    "pilot",
    [
        {},
        {"ref": "this", "on": "this"},
        {"ref": "this", "attack": 3},
        {"ref": "this", "until": "turn"},
    ],
)
def test_pilot_shape_keeps_printed_values_and_duration_implicit(
    pilot: JsonValue,
) -> None:
    _check([{"pilot": pilot}], accepted=False)


@pytest.mark.parametrize("cost", [-1, 1, False, True, "0", None])
def test_play_card_accepts_only_the_zero_cost_override(cost: JsonValue) -> None:
    _check({"ref": "t", "cost": cost}, name="A.act.play-card", accepted=False)
    _check([{"play_card": {"ref": "t", "cost": cost}}], accepted=False)
