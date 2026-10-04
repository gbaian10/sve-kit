"""Migration writes editable YAML while retaining actual current semantics."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import canonical, digest, object_value
from sve_carddb.translations.loader import load_glossary
from sve_carddb.translations.migration import glossary

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
