"""Finite symbol spellings with exact source retention and no regex execution."""

import re
from typing import TYPE_CHECKING, Literal

from jsonschema import ValidationError
from pydantic import JsonValue, model_validator

from sve_carddb.build.t0_json import schemas, symbol_valid
from sve_carddb.build.validation import Rules
from sve_carddb.core.json import array, canonical, object_value, string
from sve_carddb.core.models import RecordData, Text
from sve_carddb.domains.products.models import Code, Lang

if TYPE_CHECKING:
    from collections.abc import Mapping


class Spelling(RecordData):
    lang: Lang
    literal_prefix: str
    literal_suffix: str
    parameter_name: Code | None
    parse_kind: Literal["literal", "uint", "variable"]


class Localization(RecordData):
    lang: Lang
    name: Text
    tooltip: str
    copy_pattern: str


class Symbol(RecordData):
    id: Text
    code: Code
    parameter_schema: dict[str, JsonValue]
    keyword_id: Text | None = None
    spellings: tuple[Spelling, ...]
    localizations: tuple[Localization, ...]
    decision_id: Text

    @model_validator(mode="after")
    def validate_contract(self) -> Symbol:
        """Check the existing parameter authority and every localization reference."""
        rules = Rules(
            {key: canonical(value).decode() for key, value in schemas().items()}
        )
        spellings: JsonValue = [s.model_dump(mode="json") for s in self.spellings]
        try:
            rules.validate("text_symbol_parameter_schema", self.parameter_schema)
            rules.validate("text_symbol_spellings", spellings)
        except ValidationError:
            raise ValueError("Invalid finite symbol declaration") from None
        if not symbol_valid(
            canonical(self.parameter_schema).decode(), canonical(spellings).decode()
        ):
            raise ValueError("Invalid symbol spelling parameter domains")
        params = _parameters(self)
        langs = [item.lang for item in self.localizations]
        if len(langs) != len(set(langs)) or len(self.spellings) != len(
            set(self.spellings)
        ):
            raise ValueError("Duplicate symbol localization or spelling")
        for spelling in self.spellings:
            if spelling.parse_kind == "literal" and not (
                spelling.literal_prefix or spelling.literal_suffix
            ):
                raise ValueError("Empty symbol spelling")
        for item in self.localizations:
            for value in (item.name, item.tooltip, item.copy_pattern):
                _references(value, params)
        return self


def _parameters(symbol: Symbol) -> dict[str, dict[str, JsonValue]]:
    return {
        string(item["name"]): item
        for raw in array(symbol.parameter_schema["parameters"])
        for item in [object_value(raw)]
    }


def _references(pattern: str, parameters: Mapping[str, object]) -> None:
    for name in re.findall(r"\{([^{}]*)\}", pattern):
        if name not in parameters:
            raise ValueError("Localization references an undeclared parameter")
    remainder = re.sub(r"\{[^{}]*\}", "", pattern)
    if "{" in remainder or "}" in remainder:
        raise ValueError("Invalid localization placeholder")
