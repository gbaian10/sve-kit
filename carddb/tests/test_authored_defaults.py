"""Compact authored input preserves withdrawals, source modes and translation quality."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.catalog.current_models import Shard as CatalogShard
from sve_carddb.products.identity_models import IdentityShard
from sve_carddb.products.models import Shard as ProductShard
from sve_carddb.registry.storage import Entry, encode, read_yaml
from sve_carddb.registry.transitions.models import After, Transition
from sve_carddb.snapshot.values import array, canonical, object_value
from sve_carddb.template_translations.current_models import Shard as TemplateShard
from sve_carddb.translations.current_models import Shard, TermRecord

from .translation_fixtures import choice, name_term, term

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import BaseModel


def test_omitted_defaults_and_withdrawal_survive_yaml_round_trip(
    tmp_path: Path,
) -> None:
    concept = term()
    concept.pop("origin")
    concept.pop("low_confidence")
    object_value(concept["data"]).pop("missing_source_reason")
    target = choice(value=None)
    target.update(origin="machine", low_confidence=True)
    raw: dict[str, JsonValue] = {
        "translation_authored_format": 2,
        "kind": "translation_shard",
        "records": [concept, target],
    }
    loaded = Shard.model_validate_json(canonical(raw))
    assert loaded.records[0].origin == "project"
    assert loaded.records[0].low_confidence is False
    assert isinstance(loaded.records[0], TermRecord)
    assert loaded.records[0].data.missing_source_reason is None
    path = tmp_path / "compact.yaml"
    path.write_bytes(encode(loaded))
    stored = array(object_value(read_yaml(path))["records"])
    first, second = (object_value(value) for value in stored)
    assert {"record_key", "origin", "low_confidence"}.isdisjoint(first)
    assert "missing_source_reason" not in object_value(first["data"])
    assert "record_key" not in second
    assert second["origin"] == "machine"
    assert second["low_confidence"] is True
    assert "value" in object_value(second["data"])
    assert object_value(second["data"])["value"] is None
    assert Shard.model_validate_json(canonical(read_yaml(path))) == loaded


def test_explicit_defaults_are_accepted_but_never_written() -> None:
    record = TermRecord.model_validate_json(canonical(term()))
    assert b"origin:" not in encode(record)
    assert b"low_confidence:" not in encode(record)
    assert b"missing_source_reason:" not in encode(record)
    assert b"record_key:" not in encode(record)


def test_authored_source_reason_cannot_be_omitted() -> None:
    raw = name_term()
    reason = object_value(raw["data"])["missing_source_reason"]
    record = TermRecord.model_validate_json(canonical(raw))
    assert reason is not None
    assert b"missing_source_reason:" in encode(record)
    object_value(raw["data"]).pop("missing_source_reason")
    with pytest.raises(ValidationError, match="requires name and reason"):
        TermRecord.model_validate_json(canonical(raw))


def test_stored_key_is_rejected_even_when_it_matches_derived_identity() -> None:
    raw = term()
    raw["record_key"] = '["glossary_term","term:rule.test"]'
    with pytest.raises(ValidationError, match="record_key"):
        TermRecord.model_validate_json(canonical(raw))


@pytest.mark.parametrize(
    "model",
    [
        Entry,
        After,
        Transition,
        CatalogShard,
        IdentityShard,
        ProductShard,
        Shard,
        TemplateShard,
    ],
)
def test_validation_schema_does_not_advertise_derived_keys(
    model: type[BaseModel],
) -> None:
    schema = model.model_json_schema()
    assert "record_key" not in schema.get("properties", {})
    if model not in {After, Transition}:
        assert all(
            "record_key" not in value.get("properties", {})
            for value in schema.get("$defs", {}).values()
        )
