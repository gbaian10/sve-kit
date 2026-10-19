"""A strict JSON boundary with readable, deterministic bytes and generated schemas."""

import json
from functools import cache
from typing import TYPE_CHECKING, cast

from jsonschema_rs import Draft202012Validator

from sve_carddb.core.json import object_value, parse
from sve_carddb.domains.parsed_cards.models import Index, Shard

if TYPE_CHECKING:
    from pydantic import JsonValue


def encode(value: Index | Shard) -> bytes:
    """Use UTF-8, two-space indentation, declared field order and a final LF."""
    return (
        json.dumps(value.model_dump(mode="json"), ensure_ascii=False, indent=2) + "\n"
    ).encode()


def schema(model: type[Index | Shard]) -> dict[str, JsonValue]:
    """Close nested extractor dataclasses as well as the validated outer models."""
    value = object_value(parse(json.dumps(model.model_json_schema()).encode()))
    pending: list[JsonValue] = [value]
    while pending:
        item = pending.pop()
        if isinstance(item, dict):
            pattern = item.get("pattern")
            if isinstance(pattern, str):
                item["pattern"] = pattern.replace(r"\Z", r"(?![\s\S])")
            if item.get("type") == "object" and "properties" in item:
                item["additionalProperties"] = False
            pending.extend(item.values())
        elif isinstance(item, list):
            pending.extend(item)
    return value


@cache
def _validator(*, index: bool) -> Draft202012Validator:
    return Draft202012Validator(schema(Index if index else Shard))


def decode[T: Index | Shard](model: type[T], raw: bytes) -> T:
    """Reject duplicate keys, unknown fields, incompatible recipes and changed bytes."""
    value = parse(raw)
    # Plain extractor dataclasses also reject unknown keys at the wire boundary.
    if not _validator(index=model is Index).is_valid(value):
        raise ValueError("Parsed data does not match its schema")
    result = model.model_validate_json(raw)
    if encode(result) != raw:
        raise ValueError("Parsed data is not in its canonical readable format")
    return cast("T", result)
