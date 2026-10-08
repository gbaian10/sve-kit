"""Finite symbol spellings with exact source retention and no regex execution."""

import re
from typing import TYPE_CHECKING, Literal

from jsonschema import ValidationError
from pydantic import JsonValue, model_validator

from sve_carddb.build.t0_json import schemas, symbol_valid
from sve_carddb.build.validation import Rules
from sve_carddb.core.json import array, canonical, integer, object_value, string
from sve_carddb.core.models import RecordData, Text
from sve_carddb.products.models import Code, Lang

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


class SymbolMatch(RecordData):
    symbol_id: str | None
    raw: str
    parameters: dict[str, int | str]


def _parse(symbol: Symbol, spelling: Spelling, raw: str) -> dict[str, int | str] | None:  # ruff: ignore[too-many-return-statements] -- finite spelling domains fail closed independently
    prefix, suffix = spelling.literal_prefix, spelling.literal_suffix
    if not raw.startswith(prefix) or not raw.endswith(suffix):
        return None
    end = len(raw) - len(suffix) if suffix else len(raw)
    if end < len(prefix):
        return None
    value = raw[len(prefix) : end]
    if spelling.parse_kind == "literal":
        return {} if not value else None
    assert spelling.parameter_name is not None
    param = _parameters(symbol)[spelling.parameter_name]
    if spelling.parse_kind == "variable":
        return (
            {spelling.parameter_name: value}
            if value in array(param["variables"])
            else None
        )
    bounds = object_value(param["uint"])
    if not value or not value.isascii() or not value.isdecimal():
        return None
    # Compare decimal strings first so hostile input never triggers Python's int digit limit.
    digits = value.lstrip("0") or "0"
    maximum = str(integer(bounds["maximum"]))
    if (len(digits), digits) > (len(maximum), maximum):
        return None
    parsed = int(digits)
    return (
        {spelling.parameter_name: parsed}
        if parsed >= integer(bounds["minimum"])
        else None
    )


def parse_symbol(symbols: tuple[Symbol, ...], lang: str, raw: str) -> SymbolMatch:
    """Return literal source even when unknown, ambiguous, or leading zeros were parsed."""
    found = {
        (symbol.id, tuple(sorted(params.items())))
        for symbol in symbols
        for spelling in symbol.spellings
        if spelling.lang == lang
        and (params := _parse(symbol, spelling, raw)) is not None
        and params.keys() == _parameters(symbol).keys()
    }
    if len(found) != 1:
        return SymbolMatch(symbol_id=None, raw=raw, parameters={})
    symbol_id, items = found.pop()
    return SymbolMatch(symbol_id=symbol_id, raw=raw, parameters=dict(items))
