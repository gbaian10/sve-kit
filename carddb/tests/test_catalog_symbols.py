"""Independent finite-spelling counterexamples, entirely synthetic."""

import pytest
from pydantic import ValidationError

from sve_carddb.catalog.symbols import Localization, Spelling, Symbol, parse_symbol
from sve_carddb.snapshot.values import canonical


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
    ("raw", "value"), [("[0]", 0), ("[9]", 9), ("[007]", 7), ("[X]", "X")]
)
def test_exact_roundtrip(raw: str, value: int | str) -> None:
    found = parse_symbol((symbol(),), "ja", raw)
    assert found.symbol_id == "symbol:test"
    assert found.parameters == {"value": value}
    assert found.raw == raw


@pytest.mark.parametrize(
    "raw",
    [
        "[Q]",
        "[Y]",
        "[x]",
        "[10]",
        "[-1]",
        "[１]",
        "[1.0]",
        " [1]",
        "[1] ",
        "[]",
        "[" + "9" * 5000 + "]",
    ],
)
def test_unknown_preserves_all_bytes(raw: str) -> None:
    found = parse_symbol((symbol(),), "ja", raw)
    assert found.symbol_id is None
    assert found.raw == raw
    assert found.parameters == {}


def test_q_is_literal_and_never_a_public_variable() -> None:
    literal = symbol().model_dump(mode="json")
    literal["parameter_schema"] = {"parameters": []}
    literal["spellings"] = [
        {
            "lang": "ja",
            "literal_prefix": "Q",
            "literal_suffix": "",
            "parameter_name": None,
            "parse_kind": "literal",
        }
    ]
    literal["localizations"] = []
    item = Symbol.model_validate_json(canonical(literal))
    assert parse_symbol((item,), "ja", "Q").raw == "Q"
    assert parse_symbol((item,), "ja", "Q").symbol_id == item.id
    assert parse_symbol((item,), "en", "Q").symbol_id is None


def test_ambiguous_symbol_never_selects_by_order() -> None:
    first = symbol()
    second = first.model_copy(update={"id": "another"})
    for symbols in ((first, second), (second, first)):
        assert parse_symbol(symbols, "ja", "[X]").symbol_id is None


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
