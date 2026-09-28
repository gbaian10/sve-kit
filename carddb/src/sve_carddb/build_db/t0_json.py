"""T0 JSON shapes and finite parameter checks; no template evaluation."""

from pydantic import JsonValue

from sve_carddb.build_db.domains import CODE, LANG
from sve_carddb.snapshot.contract import definition
from sve_carddb.snapshot.values import array, integer, object_value, parse, string


def _object(properties: dict[str, JsonValue]) -> dict[str, JsonValue]:
    return {
        "type": "object",
        "properties": properties,
        "required": list(properties),
        "additionalProperties": False,
    }


def _array(item: JsonValue, *, unique: bool = False) -> dict[str, JsonValue]:
    return {"type": "array", "items": item, "uniqueItems": unique}


def schemas() -> dict[str, JsonValue]:
    """Return standalone schemas, retaining the public ParameterSchema authority."""
    code: JsonValue = {"type": "string", "pattern": "^" + CODE + r"$(?![\s\S])"}
    lang: JsonValue = {"type": "string", "pattern": "^" + LANG + r"$(?![\s\S])"}
    text: JsonValue = {"type": "string"}
    spelling = _object(
        {
            "lang": lang,
            "literal_prefix": text,
            "literal_suffix": text,
            "parameter_name": {"anyOf": [code, {"type": "null"}]},
            "parse_kind": {"enum": ["literal", "uint", "variable"]},
        }
    )
    spelling["allOf"] = [
        {
            "if": {"properties": {"parse_kind": {"const": "literal"}}},
            "then": {"properties": {"parameter_name": {"type": "null"}}},
            "else": {"properties": {"parameter_name": code}},
        }
    ]
    return {
        "decision_sample_ids": _array({"type": "string", "minLength": 1}, unique=True),
        "language_fallback_order": _array(lang, unique=True),
        "card_engine_support_reason_codes": _array(code, unique=True),
        "text_symbol_parameter_schema": {
            "$defs": {"Code": code, "UInt": definition("UInt")},
            **definition("ParameterSchema"),
        },
        "text_symbol_spellings": _array(spelling),
        "text_symbol_localizations": _array(
            _object({"lang": lang, "name": text, "tooltip": text, "copy_pattern": text})
        ),
    }


def sorted_unique(data: object) -> int:
    """Check ordered Code arrays after their JSON Schema has checked the domain."""
    if not isinstance(data, str):
        return 0
    try:
        values = [string(value) for value in array(parse(data.encode()))]
    except ValueError, TypeError:
        return 0
    return int(values == sorted(set(values)))


def _parameters(value: JsonValue) -> dict[str, dict[str, JsonValue]]:
    declaration = object_value(value)
    params = [object_value(item) for item in array(declaration["parameters"])]
    names = [string(item["name"]) for item in params]
    if names != sorted(set(names)):
        raise ValueError("Parameter names must be sorted and unique")
    for param in params:
        if param["uint"] is not None:
            bounds = object_value(param["uint"])
            if integer(bounds["minimum"]) > integer(bounds["maximum"]):
                raise ValueError("Inverted parameter range")
    return dict(zip(names, params, strict=True))


def symbol_valid(parameters: object, spellings: object) -> int:
    """Enforce parameter ordering, closed ranges and declared spelling domains."""
    if not isinstance(parameters, str) or not isinstance(spellings, str):
        return 0
    try:
        by_name = _parameters(parse(parameters.encode()))
        for raw in array(parse(spellings.encode())):
            spelling = object_value(raw)
            kind = spelling["parse_kind"]
            if kind == "literal":
                if spelling["parameter_name"] is not None:
                    return 0
                continue
            param = by_name[string(spelling["parameter_name"])]
            if (kind == "uint" and param["uint"] is None) or (
                kind == "variable" and not array(param["variables"])
            ):
                return 0
    except ValueError, TypeError, KeyError:
        return 0
    return 1
