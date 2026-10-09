"""Editable inputs retain schema, duplicate, link and immutable snapshot checks."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.inputs import load_glossary

from ...support.translation_fixtures import choice, envelope, term, write

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize("fault", ["shard_v1", "duplicate", "symlink", "area", "size"])
def test_current_input_refusals(tmp_path: Path, fault: str) -> None:
    name = "translations/glossary/shared/007.yaml"
    shard = envelope([term(), choice()])
    write(tmp_path, {name: shard})
    path = tmp_path / name
    if fault == "shard_v1":
        shard["format"] = 1
    elif fault == "duplicate":
        shard["records"] = [term(), term()]
    elif fault == "symlink":
        path.unlink()
        path.symlink_to("missing.yaml")
    elif fault == "area":
        other = tmp_path / "translations/overrides/shared/007.yaml"
        other.parent.mkdir(parents=True)
        path.rename(other)
    else:
        path.write_bytes(b" " * 1048576)
    if fault in {"shard_v1", "duplicate"}:
        path.write_bytes(canonical(shard))
    with pytest.raises(ValueError, match=r"Invalid|unique|Symlink|symlink|wrong|MiB"):
        load_glossary(tmp_path)


def test_unsorted_records_and_gapped_shards_are_sorted_after_loading(
    tmp_path: Path,
) -> None:
    name = "translations/glossary/shared/007.yaml"
    write(tmp_path, {name: envelope([term(), choice()])})
    raw = envelope([term(), choice()])
    raw["records"] = [term(), choice()]
    (tmp_path / name).write_bytes(canonical(raw))
    snapshot = load_glossary(tmp_path)
    assert [r.record_key for r in snapshot.current_records()] == sorted(
        r.record_key for r in snapshot.current_records()
    )
    before = snapshot.closure
    (tmp_path / name).write_bytes(b"invalid replacement")
    assert snapshot.closure == before
