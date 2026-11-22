"""The author tool enforces declaration timing and single token instructions."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.core.json import array, object_value
from sve_carddb.domains.effects.author_inputs import AuthorInputError

from ...support.direct_author_fixture import direct_author_input

if TYPE_CHECKING:
    from pathlib import Path


def choose(bind: str = "picked", **quantity: JsonValue) -> dict[str, JsonValue]:
    return {
        "choose": {
            "bind": bind,
            "from": {"who": "opp", "zone": "field", "kind": "follower"},
            **quantity,
        },
    }


def create(*tokens: str) -> dict[str, JsonValue]:
    return {
        "create": {
            "token": list[JsonValue](tokens) if len(tokens) > 1 else tokens[0],
            "to": "ex",
        }
    }


_DRAW: dict[str, JsonValue] = {"draw": {"n": {"param": "n"}}}
_CONDITION: dict[str, JsonValue] = {"cmp": {"left": 1, "op": "eq", "right": 1}}
_EXCEPTION: dict[str, JsonValue] = {
    "rule": "required_choose_use_targets",
    "line": 1,
    "path": "/spell/do/0/choose",
    "reason": "The printed instruction selects a card during resolution.",
}


@pytest.mark.parametrize(
    ("steps", "rule", "instruction"),
    [
        ([choose(n={"param": "n"})], "required_choose_use_targets", "targets"),
        (
            [
                _DRAW,
                {
                    "optional": {
                        "do": [create("Falcon"), create("Owl")],
                    }
                },
            ],
            "combine_consecutive_create",
            "token list",
        ),
        (
            [_DRAW, create("Falcon", "Owl"), create("Falcon", "Owl")],
            "combine_consecutive_create",
            "token list",
        ),
    ],
)
def test_author_write_rejects_ambiguous_forms(
    tmp_path: Path, steps: list[JsonValue], rule: str, instruction: str
) -> None:
    with pytest.raises(AuthorInputError) as caught:
        direct_author_input(tmp_path, steps=steps)
    diagnostic = caught.value.diagnostic
    assert (diagnostic.stage, diagnostic.reason, diagnostic.card_no) == (
        "lint",
        rule,
        "SYN-01",
    )
    assert diagnostic.card_line == 1
    assert diagnostic.expected is not None
    assert instruction in diagnostic.expected


def test_author_write_allows_conditional_optional_and_batched_steps(
    tmp_path: Path,
) -> None:
    steps: list[JsonValue] = [
        _DRAW,
        {"if": {"cond": _CONDITION, "then": [choose(n=1)]}},
        {"choice": {"n": 1, "options": [[choose(n=1)], [_DRAW]]}},
        {"optional": {"do": [choose(n=1)]}},
        {"pay": {"cost": {"pp": 1}, "do": [choose(n=1)]}},
        choose(upto=1),
        choose(any=True),
        choose(n=1, may=True),
        create("Falcon", "Owl"),
        _DRAW,
        create("Falcon"),
        create("Falcon"),
    ]
    selection = object_value(direct_author_input(tmp_path, steps=steps))
    assert object_value(array(selection["authors"])[0])["status"] == "accepted"


def test_author_write_applies_exception_only_to_its_step(tmp_path: Path) -> None:
    required = choose(n={"param": "n"})
    selection = object_value(
        direct_author_input(
            tmp_path / "accepted", steps=[required], lint_exceptions=[_EXCEPTION]
        )
    )
    handoff = object_value(object_value(array(selection["authors"])[0])["input"])
    card = object_value(object_value(object_value(handoff["tree"])["cards"])["SYN-01"])
    assert "lint_exceptions" not in card

    with pytest.raises(AuthorInputError) as caught:
        direct_author_input(
            tmp_path / "second",
            steps=[required, choose("other", n=1)],
            lint_exceptions=[_EXCEPTION],
        )
    assert caught.value.diagnostic.reason == "required_choose_use_targets"
    assert caught.value.diagnostic.path[-3:] == ("do", 1, "choose")


def test_author_write_rejects_stale_exception(tmp_path: Path) -> None:
    with pytest.raises(AuthorInputError) as caught:
        direct_author_input(tmp_path, lint_exceptions=[_EXCEPTION])
    assert caught.value.diagnostic.reason == "unused_lint_exception"
