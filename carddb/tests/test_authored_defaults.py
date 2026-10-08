"""Compact authored input preserves withdrawals, source modes and translation quality."""

from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains.catalog.records import Shard as CatalogShard
from sve_carddb.domains.products.identity_models import IdentityShard
from sve_carddb.domains.products.models import Shard as ProductShard
from sve_carddb.domains.registry.storage import (
    Entry,
    Index,
    encode,
    read_base_files,
    read_yaml,
)
from sve_carddb.domains.registry.storage import Shard as RegistryShard
from sve_carddb.domains.registry.transitions.models import After, Transition
from sve_carddb.domains.translations.glossary.records import Shard, TermRecord
from sve_carddb.domains.translations.templates.records import Shard as TemplateShard

from .translation_fixtures import choice, name_term, term

if TYPE_CHECKING:
    from pydantic import BaseModel


def _assert_no_authored_defaults(value: JsonValue, location: str) -> None:
    if isinstance(value, dict):
        defaults: dict[str, JsonValue] = {
            "origin": "project",
            "low_confidence": False,
            "missing_source_reason": None,
        }
        for key, item in value.items():
            if (
                key in defaults
                and type(item) is type(defaults[key])
                and item == defaults[key]
            ):
                pytest.fail(
                    f"{location}.{key} explicitly stores a default", pytrace=False
                )
            _assert_no_authored_defaults(item, f"{location}.{key}")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            _assert_no_authored_defaults(item, f"{location}[{index}]")


def test_authored_tree_does_not_store_quality_defaults() -> None:
    root = Path(__file__).resolve().parents[2] / "authored"
    paths = sorted(root.rglob("*.yaml"))
    assert paths, "Authored tree must contain YAML files"
    checked = 0
    for path in paths:
        raw = read_yaml(path)
        # Other formats require quality/proof fields; only these shards declare omitted defaults.
        if not isinstance(raw, dict) or raw.get("kind") not in {
            "translation_shard",
            "catalog_adoption_shard",
            "display_override_shard",
        }:
            continue
        checked += 1
        _assert_no_authored_defaults(raw, path.relative_to(root).as_posix())
    assert checked > 0, "Authored tree must contain shards with quality defaults"


@pytest.mark.parametrize(
    ("kind", "identity"),
    [
        ("card", "id"),
        ("card_int_id", "printing_id"),
        ("region_mapping_review", "card_id"),
    ],
)
@pytest.mark.parametrize(
    "invalid",
    [{}, {"value": None}, {"value": 1}, {"value": False}, {"value": []}, {"value": {}}],
)
def test_registry_key_requires_a_string_identity(
    kind: str, identity: str, invalid: dict[str, JsonValue]
) -> None:
    data = {identity: invalid["value"]} if "value" in invalid else {}
    entry = Entry.model_validate({"kind": kind, "owner": "BP01", "data": data})
    with pytest.raises(ValueError, match=f"Registry field {identity} must be a string"):
        assert entry.record_key


def test_invalid_identity_fails_before_duplicate_registry_keys(tmp_path: Path) -> None:
    (tmp_path / "ids").mkdir()
    (tmp_path / "ids" / "index.yaml").write_bytes(encode(Index()))
    directory = tmp_path / "registry" / "card" / "BP01"
    directory.mkdir(parents=True)
    entry = Entry(kind="card", owner="BP01", data={})
    (directory / "001.yaml").write_bytes(encode(RegistryShard(records=[entry, entry])))
    with pytest.raises(ValueError, match="Registry field id must be a string"):
        read_base_files(tmp_path)


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
