"""Optional empty notes stay omitted through the shared authored YAML writer."""

import json
from typing import TYPE_CHECKING

import pytest
from pydantic import BaseModel

from sve_carddb.products.models import Shard
from sve_carddb.registry.storage import encode, read_yaml
from sve_carddb.template_parameter_rules.current import Rule, Rules, parse
from sve_carddb.template_parameters.rule_candidates import BY_ID

from .product_fixtures import decision, envelope, family

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
    rules = Rules(
        parameter_rule_format=2, kind="template_parameter_rules", rules=(rule,)
    )
    raw = encode(rules)
    assert (b"note:" in raw) == bool(note)
    assert b"enabled: false" in raw
    assert b"low_confidence: false" in raw
    assert parse(raw) == rules
    assert encode(parse(raw)) == raw


@pytest.mark.parametrize("note", ["", "Keep this explanation"])
def test_product_decision_round_trip_omits_only_empty_note(
    tmp_path: Path, note: str
) -> None:
    value = envelope([family("BP02")])
    decision(value)["note"] = note
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


def test_required_note_and_other_defaults_are_preserved() -> None:
    raw = encode(RequiredNote(note=""))
    assert b"note: ''" in raw
    assert b"enabled: false" in raw
