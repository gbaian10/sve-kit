"""Author syntax acceptance is independent of backend execution admission."""

from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import ValidationError

from sve_carddb.core.json import canonical

from .author_ability_types import ABILITIES
from .test_author_schema_drift import NODES

if TYPE_CHECKING:
    from pydantic import JsonValue


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
        pytest.param(
            {
                "static": {
                    "while": {"my_turn": True},
                    "effects": [
                        {"grant": {"keywords": ["ward"]}},
                    ],
                }
            },
            True,
            id="static-whole-head-condition",
        ),
        pytest.param(
            {
                "static": {
                    "effects": [
                        {
                            "if": {
                                "cond": {"my_turn": True},
                                "then": [{"grant": {"keywords": ["ward"]}}],
                            }
                        }
                    ]
                }
            },
            True,
            id="static-per-grant-condition",
        ),
        pytest.param(
            {
                "static": {
                    "effects": [
                        {
                            "if": {
                                "cond": {"my_turn": True},
                                "then": [
                                    {"prevent": {"kind": "ability", "on": "this"}}
                                ],
                            }
                        }
                    ]
                }
            },
            True,
            id="static-prevention-exception",
        ),
    ],
)
def test_author_syntax_acceptance_at_consumer_sites(
    head: JsonValue, *, accepted: bool
) -> None:
    _assert_acceptance(head, accepted=accepted)


def _assert_acceptance(head: JsonValue, *, accepted: bool) -> None:
    schema = {
        "$id": NODES["$id"],
        "$defs": NODES["$defs"],
        "$ref": "#/$defs/A.ability-head",
    }
    assert Draft202012Validator(schema).is_valid(head) is accepted
    adapter = ABILITIES["A.ability-head"]
    if accepted:
        decoded = adapter.validate_json(canonical(head), strict=True)
        assert canonical(adapter.dump_python(decoded, mode="json")) == canonical(head)
    else:
        with pytest.raises(ValidationError):
            adapter.validate_json(canonical(head), strict=True)


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
            {"cond": {"my_turn": True}, "then": []},
            False,
            id="empty-then",
        ),
        pytest.param(
            {
                "cond": {"my_turn": True},
                "then": [{"grant": {"keywords": ["ward"]}}] * 2,
            },
            False,
            id="multiple-declarations-in-one-branch",
        ),
        pytest.param(
            {"cond": {"my_turn": True}, "then": [{"grant": {}}]},
            False,
            id="empty-grant",
        ),
        pytest.param(
            {
                "cond": {"my_turn": True},
                "then": [{"grant": {"keywords": ["ward"]}}],
                "else": [{"grant": {"keywords": ["storm"]}}],
            },
            False,
            id="else-branch",
        ),
        pytest.param(
            {
                "cond": {"my_turn": True},
                "then": [
                    {
                        "if": {
                            "cond": {"awakened": True},
                            "then": [{"grant": {"keywords": ["ward"]}}],
                        }
                    }
                ],
            },
            False,
            id="nested-conditional",
        ),
        pytest.param(
            {"cond": {"my_turn": True}, "then": [{"cost_mod": {"delta": -1}}]},
            False,
            id="other-effect-family",
        ),
        pytest.param(
            {
                "cond": {"my_turn": True},
                "then": [{"prevent": {"kind": "ability", "on": "this"}}] * 2,
            },
            False,
            id="prevention-count-unchanged",
        ),
        pytest.param(
            {"cond": {"my_turn": True}, "then": [{"prevent": {"kind": "battle"}}]},
            False,
            id="prevention-kind-unchanged",
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
