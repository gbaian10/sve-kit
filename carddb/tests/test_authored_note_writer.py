"""Optional empty notes stay omitted through the shared authored YAML writer."""

import json
from typing import TYPE_CHECKING

import pytest
from pydantic import BaseModel

from sve_carddb.core.yaml import parse_yaml
from sve_carddb.domains.products.models import Shard
from sve_carddb.domains.registry.storage import encode, read_yaml
from sve_carddb.domains.translations.parameters.rule_candidates import BY_ID
from sve_carddb.domains.translations.parameters.rules import Rule, Rules, parse

from .product_fixtures import envelope, family, first_record

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize("note", ["", "Keep this explanation"])
def test_parameter_rules_round_trip_omits_only_empty_note(note: str) -> None:
    rule = Rule(
        rule_id=next(iter(BY_ID)),
        enabled=False,
        origin="project",
        low_confidence=False,
        note=note,
    )
    rules = Rules(format=2, kind="template_parameter_rules", rules=(rule,))
    raw = encode(rules)
    assert (b"note:" in raw) == bool(note)
    assert b"enabled: false" in raw
    assert b"low_confidence: false" in raw
    assert parse(raw) == rules
    assert encode(parse(raw)) == raw


@pytest.mark.parametrize("note", ["", "Keep this explanation"])
def test_product_record_round_trip_omits_only_empty_note(
    tmp_path: Path, note: str
) -> None:
    value = envelope([family("BP02")])
    first_record(value)["note"] = note
    original = Shard.model_validate_json(json.dumps(value))
    raw = encode(original)
    assert (b"note:" in raw) == bool(note)
    path = tmp_path / "product.yaml"
    path.write_bytes(raw)
    restored = Shard.model_validate_json(json.dumps(read_yaml(path)))
    assert restored == original
    assert encode(restored) == raw


class RequiredNote(BaseModel):
    note: str
    enabled: bool = False


class NullableValues(BaseModel):
    required: str | None
    optional: str | None = None


def test_required_null_and_optional_value_round_trip() -> None:
    original = NullableValues(required=None, optional="Keep this value")
    raw = encode(original)
    assert b"required:" in raw
    assert b"optional: Keep this value" in raw
    assert NullableValues.model_validate_json(json.dumps(parse_yaml(raw))) == original

    empty = NullableValues(required=None)
    raw = encode(empty)
    assert b"required:" in raw
    assert b"optional:" not in raw
    assert NullableValues.model_validate_json(json.dumps(parse_yaml(raw))) == empty


class CollectionDefaults(BaseModel):
    items: list[str] = []
    labels: dict[str, str] = {}


def test_collection_defaults_remain_serializable() -> None:
    original = CollectionDefaults()
    raw = encode(original)
    assert b"items: []" in raw
    assert b"labels: {}" in raw
    assert (
        CollectionDefaults.model_validate_json(json.dumps(parse_yaml(raw))) == original
    )


def test_required_note_and_other_defaults_are_preserved() -> None:
    raw = encode(RequiredNote(note=""))
    assert b"note: ''" in raw
    assert b"enabled: false" in raw
