"""Migration writes editable YAML while retaining actual current semantics."""

from typing import TYPE_CHECKING
from unittest.mock import Mock

from pydantic import JsonValue

from sve_carddb.catalog.adoption_loader import AdoptionSnapshot, load_adoptions
from sve_carddb.catalog.adoption_models import SourceRef, TextEvidence
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import canonical, digest, object_value
from sve_carddb.translations.loader import load_glossary
from sve_carddb.translations.migration import catalog, glossary

from .adoption_fixtures import make_case
from .test_translation_current_values import _records, _write

if TYPE_CHECKING:
    from pathlib import Path


def test_migration_is_editable_reproducible_and_keeps_quality(tmp_path: Path) -> None:
    records = _records()
    records[1]["origin"] = "machine"
    records[1]["low_confidence"] = True
    _write(tmp_path, records)
    source = load_glossary(tmp_path)
    converted = glossary(source)
    assert converted == glossary(source)
    index = object_value(read_yaml(tmp_path / "translations/index.yaml"))
    for name in object_value(index["includes"]):
        (tmp_path / name).unlink()
    includes: dict[str, JsonValue] = {}
    for name, raw in converted.items():
        assert raw.startswith(b"translation_authored_format: 2\n")
        assert b"\nrecords:\n  - record_key:" in raw
        path = tmp_path / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(raw)
        includes[name] = digest(canonical(read_yaml(path)))
    index["includes"] = includes
    (tmp_path / "translations/index.yaml").write_bytes(canonical(index))
    assert load_glossary(tmp_path).current_records() == source.current_records()


def test_catalog_migration_drops_private_event_roles_only(tmp_path: Path) -> None:
    case = make_case(tmp_path / "repo")
    original = next(
        r
        for r, _ in load_adoptions(case.root, entry="catalog-adoptions").effective()
        if r.kind == "vocabulary_adoption"
    )
    ref = SourceRef(
        store_id="synthetic-store",
        batch_id=digest(b"synthetic-batch"),
        source_version_id="src:v1:" + digest(b"synthetic-version")[7:],
        parser="exact-json-v1",
        locator="/value",
        text_hash=digest(b"Synthetic value"),
    )
    record = original.model_copy(
        update={
            "evidence": tuple(
                TextEvidence(source_ref=ref, role=role)
                for role in (
                    "依確認頁的維護者本人按鍵",
                    "依第 2 版完整確認事件",
                )
            )
        }
    )
    snapshot = Mock(spec=AdoptionSnapshot)
    snapshot.effective.return_value = ((record, "synthetic-decision"),)
    path, raw = next(iter(catalog(snapshot).items()))
    target = tmp_path / path
    target.parent.mkdir(parents=True)
    target.write_bytes(raw)
    from sve_carddb.catalog.current_models import Shard  # ruff: ignore[import-outside-top-level] -- distinguish catalog shard from the glossary shard in this case

    migrated = Shard.model_validate_json(canonical(read_yaml(target))).records[0]
    assert migrated.data.subject.model_dump() == original.data.subject.model_dump()
    assert migrated.data.value is not None
    assert original.data.value is not None
    assert migrated.data.value.model_dump() == original.data.value.model_dump()
    assert len(migrated.evidence) == 2
    assert all(
        isinstance(e, TextEvidence) and e.source_ref == ref for e in migrated.evidence
    )
    assert all(
        not any(token in e.role for token in ("確認頁", "按鍵", "事件"))
        and "精確詞彙原值" in e.role
        for e in migrated.evidence
    )
    assert record.evidence[0].role == "依確認頁的維護者本人按鍵"
