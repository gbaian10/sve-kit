"""Admission owns historical integrity; ordinary builds only deserialize the input."""

import json
import shutil
from pathlib import Path

import pytest

from sve_carddb.core.json import digest
from sve_carddb.domains.card_extras.reviewed_errata import (
    Block,
    configuration,
    read,
    validate,
)
from sve_carddb.domains.card_extras.reviewed_errata_verify import verify_ranges

ROOT = Path(__file__).parents[4] / "card-data"


def candidate(tmp_path: Path) -> Path:
    root = tmp_path / "candidate"
    shutil.copytree(ROOT / "errata", root / "errata")
    return root


def repin(root: Path) -> None:
    path = root / "errata" / "index.json"
    value = json.loads(path.read_bytes())
    for pin in value["files"]:
        raw = (path.parent / pin["path"]).read_bytes()
        pin.update(sha256=digest(raw), bytes=len(raw))
    path.write_text(json.dumps(value))


def test_all_reviewed_announcements_are_available_without_archive(
    tmp_path: Path,
) -> None:
    root = candidate(tmp_path)
    summary = validate(root)
    pages = read(root, store_id="test-archive")
    assert summary["notices"] == len(pages) > 0
    assert summary["printings"] > 0
    assert {p.region for p in pages} == {"jp", "en"}
    assert all(p.announced_on is not None and p.reason is not None for p in pages)
    assert all(p.effective_on is None for p in pages)
    assert all(item.scope == "listed" for p in pages for item in p.printings)
    assert all(p.source.archive.store_id == "test-archive" for p in pages)
    illustration = next(p for p in pages if p.official_url.endswith("cp03-004/"))
    assert not illustration.changes


def test_admission_rejects_modified_and_removed_history(tmp_path: Path) -> None:
    root = candidate(tmp_path)
    previous = tmp_path / "previous"
    shutil.copytree(root, previous)
    path = root / "errata" / "jp.json"
    value = json.loads(path.read_bytes())
    value["notices"][0]["announced_on"] = "2023-02-21"
    path.write_text(json.dumps(value))
    with pytest.raises(ValueError, match="hash/size"):
        validate(root)
    repin(root)
    with pytest.raises(ValueError, match="history changed"):
        validate(root, previous=previous)
    shutil.rmtree(root / "errata")
    with pytest.raises(ValueError, match="cannot be removed"):
        validate(root, previous=previous)


def test_build_reader_does_not_repeat_admission(tmp_path: Path) -> None:
    root = candidate(tmp_path)
    (root / "errata" / "index.json").unlink()
    assert read(root, store_id="test-archive")
    with pytest.raises(FileNotFoundError):
        validate(root)


def test_admission_rejects_duplicate_keys_and_index_symlink(tmp_path: Path) -> None:
    root = candidate(tmp_path)
    path = root / "errata" / "jp.json"
    path.write_bytes(
        path.read_bytes().replace(b'"format": 1', b'"format": 0, "format": 1', 1)
    )
    repin(root)
    with pytest.raises(ValueError, match="Duplicate"):
        validate(root)
    index = root / "errata" / "index.json"
    target = tmp_path / "index.json"
    index.rename(target)
    index.symlink_to(target)
    with pytest.raises(ValueError, match="symlink"):
        validate(root)


def test_configuration_records_uncommitted_bytes_without_admission(
    tmp_path: Path,
) -> None:
    root = candidate(tmp_path)
    before = configuration(root)
    path = root / "errata" / "jp.json"
    path.write_bytes(path.read_bytes() + b"\n")
    assert configuration(root) != before


@pytest.mark.parametrize("invalid", ["bounds", "hash"])
def test_evidence_byte_range_verifier_rejects_corruption(invalid: str) -> None:
    raw = "前後".encode()
    block: dict[str, object] = {
        "ordinal": 0,
        "context": [],
        "before": "前",
        "after": "後",
        "raw_before": {"start": 0, "end": 3, "sha256": digest(raw[:3])},
        "raw_after": {"start": 3, "end": 6, "sha256": digest(raw[3:])},
    }
    assert verify_ranges(raw, (Block.model_validate_json(json.dumps(block)),)) == 2
    if invalid == "bounds":
        span = block["raw_after"]
        assert isinstance(span, dict)
        span["end"] = 7
    else:
        span = block["raw_after"]
        assert isinstance(span, dict)
        span["sha256"] = digest(b"different")
    with pytest.raises(ValueError, match="range"):
        verify_ranges(raw, (Block.model_validate_json(json.dumps(block)),))
