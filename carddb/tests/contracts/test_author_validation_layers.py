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
            False,
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
