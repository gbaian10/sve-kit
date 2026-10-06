"""Current-only loading preserves complete file and canonical hash boundaries."""

import re
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.snapshot.values import canonical, digest, object_value
from sve_carddb.translations.loader import load_glossary

from .translation_fixtures import choice, envelope, term, write

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        (
            "index_v1",
            "Invalid translation authored fields at translation_authored_format",
        ),
        (
            "shard_v1",
            "Invalid translation authored fields at translation_authored_format",
        ),
        ("missing", "Translation indexed file closure differs from disk"),
        ("extra", "Translation indexed file closure differs from disk"),
        ("symlink", "Symlink translation input"),
        ("hash", "Translation shard hash mismatch"),
        ("path", "Unsafe or unsupported translation include"),
        ("area", "Translation record is in the wrong authored area"),
        ("unsorted", "Current translation records must be sorted and unique"),
        ("duplicate", "Current translation records must be sorted and unique"),
    ],
)
def test_current_closure_refusals(tmp_path: Path, fault: str, message: str) -> None:
    name = "translations/glossary/shared/001.yaml"
    shard = envelope([term(), choice()])
    index: dict[str, JsonValue] = {
        "translation_authored_format": 2,
        "kind": "translation_index",
        "includes": {name: digest(canonical(shard))},
    }
    write(tmp_path, {name: shard})
    path = tmp_path / name
    if fault == "index_v1":
        index["translation_authored_format"] = 1
    elif fault == "shard_v1":
        shard["translation_authored_format"] = 1
    elif fault == "missing":
        path.unlink()
    elif fault == "extra":
        (path.parent / "002.yaml").write_bytes(canonical(shard))
    elif fault == "symlink":
        path.unlink()
        path.symlink_to("missing.yaml")
    elif fault == "hash":
        object_value(index["includes"])[name] = digest(b"wrong")
    elif fault in {"path", "area"}:
        new_name = (
            "translations/other/shared/001.yaml"
            if fault == "path"
            else "translations/overrides/shared/001.yaml"
        )
        other = tmp_path / new_name
        other.parent.mkdir(parents=True)
        path.rename(other)
        index["includes"] = {new_name: digest(canonical(shard))}
    elif fault == "unsorted":
        shard["records"] = [term(), choice()]
    else:
        shard["records"] = [term(), term()]
    if fault in {"shard_v1", "unsorted", "duplicate"}:
        path.write_bytes(canonical(shard))
        object_value(index["includes"])[name] = digest(canonical(shard))
    (tmp_path / "translations/index.yaml").write_bytes(canonical(index))
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        load_glossary(tmp_path)


def test_snapshot_keeps_exact_bytes_after_current_files_change(tmp_path: Path) -> None:
    name = "translations/glossary/shared/001.yaml"
    write(tmp_path, {name: envelope([term(), choice()])})
    snapshot = load_glossary(tmp_path)
    before = snapshot.current_records()
    pins = snapshot.pins()
    (tmp_path / name).write_bytes(b"invalid replacement")
    assert snapshot.current_records() is before
    assert snapshot.pins() == pins
