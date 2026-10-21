"""Source, port and writer shapes do not establish review or execution admission."""

import json

import pytest
from pydantic import ConfigDict, RootModel, TypeAdapter, ValidationError

from sve_carddb.contracts.author_evidence import WrittenBy
from sve_carddb.contracts.author_queries import NumericExpr
from sve_carddb.domains.effects.author_validation import Violation, inline_parameters


def test_inline_visitor_preserves_alias_and_root_paths() -> None:
    adapter: TypeAdapter[NumericExpr] = TypeAdapter(
        NumericExpr, config=ConfigDict(regex_engine="python-re")
    )
    expression = adapter.validate_json('{"max":[{"minus":[1,{"param":"offset"}]}]}')
    wrapped = RootModel[object](expression)
    assert list(inline_parameters(wrapped, ("define", "X"))) == [
        Violation("inline_param_reference", ("define", "X", "max", 0, "minus", 1))
    ]
    assert list(inline_parameters({"param": "not a typed reference"}, ())) == []


@pytest.mark.parametrize(
    "date", ["2026-02-29", "0000-01-01", "2026-10-21\n", "2026-1-01"]
)
def test_writer_date_is_real_exact_calendar_date(date: str) -> None:
    with pytest.raises(ValidationError):
        WrittenBy.model_validate_json(
            json.dumps(
                {
                    "actor": {"who": "fixture", "model": None},
                    "date": date,
                }
            )
        )
