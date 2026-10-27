"""Independent finite-spelling counterexamples, entirely synthetic."""

import pytest
from pydantic import ValidationError

from sve_carddb.core.json import canonical
from sve_carddb.domains.catalog.symbols import Localization, Spelling, Symbol


def symbol() -> Symbol:
    return Symbol(
        id="symbol:test",
        code="synthetic_parameter",
        decision_id="decision",
        parameter_schema={
            "parameters": [
                {
                    "name": "value",
                    "uint": {"minimum": 0, "maximum": 9},
                    "variables": ["X"],
                }
            ]
        },
        spellings=tuple(
            Spelling(
                lang="ja",
                literal_prefix="[",
                literal_suffix="]",
                parameter_name="value",
                parse_kind=kind,
            )
            for kind in ("uint", "variable")
        ),
        localizations=(
            Localization(
                lang="ja",
                name="Synthetic {value}",
                tooltip="Value {value}",
                copy_pattern="[{value}]",
            ),
        ),
    )


@pytest.mark.parametrize(
    "mutation",
    [
        "q_variable",
        "inverted",
        "unknown_parameter",
        "format_expression",
        "empty_literal",
        "unknown_key",
        "disabled_uint",
        "duplicate_language",
    ],
)
def test_invalid_declarations_fail_before_projection(mutation: str) -> None:
    data = symbol().model_dump(mode="json")
    if mutation == "q_variable":
        data["parameter_schema"]["parameters"][0]["variables"] = ["Q"]
    elif mutation == "inverted":
        data["parameter_schema"]["parameters"][0]["uint"]["minimum"] = 10
    elif mutation == "unknown_parameter":
        data["spellings"][0]["parameter_name"] = "missing"
    elif mutation == "format_expression":
        data["localizations"][0]["copy_pattern"] = "{value.__class__}"
    elif mutation == "empty_literal":
        data["spellings"][0].update(
            parse_kind="literal",
            parameter_name=None,
            literal_prefix="",
            literal_suffix="",
        )
    elif mutation == "unknown_key":
        data["parameter_schema"]["regex"] = ".*"
    elif mutation == "disabled_uint":
        data["parameter_schema"]["parameters"][0]["uint"] = None
    else:
        data["localizations"].append(data["localizations"][0])
    with pytest.raises((ValueError, ValidationError)):
        Symbol.model_validate_json(canonical(data))
