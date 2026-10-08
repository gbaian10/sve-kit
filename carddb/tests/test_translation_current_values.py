"""Current glossary values keep semantics without receipt or revision gates."""

import copy
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.core.json import canonical, digest, object_value
from sve_carddb.domains.registry.storage import read_yaml
from sve_carddb.domains.translations.glossary.records import (
    ChoiceRecord,
    Shard,
    TermRecord,
)
from sve_carddb.domains.translations.glossary.validate import semantic_hash
from sve_carddb.domains.translations.inputs import load_glossary
from sve_carddb.domains.translations.models import AuthoredValue

from .translation_fixtures import choice, term

if TYPE_CHECKING:
    from pathlib import Path


def _write(root: Path, records: list[dict[str, JsonValue]]) -> None:
    path = root / "translations/glossary/shared/001.yaml"
    path.parent.mkdir(parents=True, exist_ok=True)
    ordered = records
    raw: dict[str, JsonValue] = {
        "translation_authored_format": 2,
        "kind": "translation_shard",
        "records": list[JsonValue](ordered),
    }
    path.write_bytes(canonical(raw))
    (root / "translations/index.yaml").write_bytes(
        canonical(
            {
                "translation_authored_format": 2,
                "kind": "translation_index",
                "includes": {
                    "translations/glossary/shared/001.yaml": digest(canonical(raw))
                },
            }
        )
    )


def _records() -> list[dict[str, JsonValue]]:
    return [
        r.model_dump(mode="json", round_trip=True)
        for r in (
            TermRecord.model_validate_json(canonical(term())),
            ChoiceRecord.model_validate_json(canonical(choice())),
        )
    ]


def test_current_choice_is_editable_without_receipt_or_revision(tmp_path: Path) -> None:
    records = _records()
    _write(tmp_path, records)
    first = load_glossary(tmp_path).current_records()
    object_value(records[1]["data"])["value"] = {
        "kind": "authored",
        "text": "新的合成譯詞",
    }
    records[1]["low_confidence"] = True
    records[1]["note"] = "尚待校對。"
    _write(tmp_path, records)
    second = load_glossary(tmp_path).current_records()
    assert [r.record_key for r in first] == [r.record_key for r in second]
    selected = next(r for r in second if r.kind == "glossary_choice")
    assert selected.low_confidence
    assert isinstance(selected.data.value, AuthoredValue)
    assert selected.data.value.text == "新的合成譯詞"
    assert "adoption_review" not in selected.data.model_dump()


def test_note_does_not_change_semantic_dependency(tmp_path: Path) -> None:
    records = _records()
    _write(tmp_path, records)
    before = {
        r.record_key: semantic_hash(r)
        for r in load_glossary(tmp_path).current_records()
    }
    for record in records:
        record["note"] = "修正說明。"
    _write(tmp_path, records)
    after = {
        r.record_key: semantic_hash(r)
        for r in load_glossary(tmp_path).current_records()
    }
    assert after == before


def test_withdrawal_is_not_replaced_by_an_old_choice(tmp_path: Path) -> None:
    records = _records()
    object_value(records[1]["data"])["value"] = None
    _write(tmp_path, records)
    assert (
        next(
            r
            for r in load_glossary(tmp_path).current_records()
            if r.kind == "glossary_choice"
        ).data.value
        is None
    )


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("duplicate", "Duplicate current translation selection key"),
        ("absent_term", "Glossary choice references an absent concept"),
        (
            "wrong_key",
            "Invalid translation authored fields at records.1.glossary_choice.record_key",
        ),
        ("official", "Official choice lacks same-concept evidence"),
    ],
)
def test_current_refusals(tmp_path: Path, change: str, message: str) -> None:
    records = _records()
    if change == "duplicate":
        # Duplicate keys in different, individually sorted shards exercise global uniqueness.
        _write(tmp_path, records)
        other = tmp_path / "translations/glossary/shared/002.yaml"
        other.write_bytes(
            canonical(
                {
                    "translation_authored_format": 2,
                    "kind": "translation_shard",
                    "records": [records[1]],
                }
            )
        )
        index = object_value(read_yaml(tmp_path / "translations/index.yaml"))
        object_value(index["includes"])["translations/glossary/shared/002.yaml"] = (
            digest(canonical(read_yaml(other)))
        )
        (tmp_path / "translations/index.yaml").write_bytes(canonical(index))
    else:
        if change == "absent_term":
            records = [records[1]]
        elif change == "wrong_key":
            records[1]["record_key"] = '["glossary_choice","term:absent","zh-Hant"]'
        else:
            records[1]["origin"] = "official"
        _write(tmp_path, records)
    with pytest.raises(ValueError, match="^" + message + "$"):
        load_glossary(tmp_path)


def test_private_approval_fields_are_not_current_data() -> None:
    raw: dict[str, JsonValue] = {
        "translation_authored_format": 2,
        "kind": "translation_shard",
        "records": list[JsonValue](_records()),
    }
    bad = copy.deepcopy(raw)
    bad["decisions"] = []
    with pytest.raises(ValueError, match="decisions"):
        Shard.model_validate_json(canonical(bad))
