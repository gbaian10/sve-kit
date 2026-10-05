"""Registry envelope preservation and independent build-input counterexamples."""

import hashlib
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.registry.inputs import digest
from sve_carddb.registry.records import CorrectionData, EnglishPrintingData
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.registry.storage import Index, encode, load, read_yaml, yaml_parser

from .registry_snapshot_fixtures import edit_record, kind_shard
from .registry_snapshot_fixtures import registry_root as registry_root  # ruff: ignore[useless-import-alias] -- expose synthetic pytest fixture
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- expose dependency of the synthetic registry fixture

if TYPE_CHECKING:
    from sve_carddb.registry.snapshot import RegistrySnapshot
    from sve_carddb.registry.storage import Entry


def test_preserves_complete_global_envelopes(registry_root: Path) -> None:
    before = {p: p.read_bytes() for p in registry_root.rglob("*.yaml")}
    snapshot = load_registry(registry_root)
    index, legacy = load(registry_root)
    assert snapshot.files.index() == index
    assert set(snapshot.records) == set(legacy)
    for shard in snapshot.files.shards:
        assert shard.content_hash == digest(read_yaml(registry_root / shard.path))
        assert (
            shard.content_hash == "sha256:" + hashlib.sha256(shard.content).hexdigest()
        )
        envelope = shard.envelope()
        for entry in envelope.records:
            record = snapshot.records[entry.record_key]
            assert record.entry() == entry
            assert record.shard_path == shard.path
    english = [
        r for r in snapshot.records.values() if isinstance(r.data, EnglishPrintingData)
    ]
    assert len(english) == 2
    assert snapshot.files.index().next_int_id == {"en": 60003, "jp": 20003}
    corrections = [
        r.data for r in snapshot.records.values() if isinstance(r.data, CorrectionData)
    ]
    assert {r.state for r in corrections} == {"active", "needs_review"}
    assert before == {p: p.read_bytes() for p in before}


def test_snapshot_cannot_be_changed_through_legacy_copies(registry_root: Path) -> None:
    snapshot = load_registry(registry_root)
    record = next(iter(snapshot.records.values()))
    record.entry().data.clear()
    snapshot.files.index().next_int_id.clear()
    snapshot.files.shards[0].envelope().records.clear()
    assert record.entry().data
    assert snapshot.files.index().next_int_id
    assert snapshot.files.shards[0].envelope().records
    with pytest.raises(TypeError):
        snapshot.records["changed"] = record  # type: ignore[index]  # exercise runtime immutability
    with pytest.raises(ValidationError, match="frozen"):
        record.data.extra = "changed"  # type: ignore[attr-defined]  # exercise runtime immutability


def test_invalid_en_member_is_not_hidden_by_jp_scope(registry_root: Path) -> None:
    def damage(entry: Entry) -> None:
        entry.data["source_face_map"] = []

    edit_record(registry_root, "printing", damage, region="en")
    with pytest.raises(ValueError, match="face mapping"):
        load_registry(registry_root)


@pytest.mark.parametrize("region", ["jp", "en"])
@pytest.mark.parametrize("direction", [-1, 1])
def test_full_global_cursors(registry_root: Path, region: str, direction: int) -> None:
    path = registry_root / "ids/index.yaml"
    index = Index.model_validate(read_yaml(path))
    index.next_int_id[region] += direction
    path.write_bytes(encode(index))
    with pytest.raises(ValueError, match="high-water"):
        load_registry(registry_root)


def test_every_shard_on_disk_is_read(registry_root: Path) -> None:
    path, _ = kind_shard(registry_root, "card")
    (path.parent / "999.yaml").write_text("not: a valid shard\n")
    with pytest.raises(ValueError, match="envelope fields or format"):
        load_registry(registry_root)


def test_missing_allocation_shard_is_detected(registry_root: Path) -> None:
    kind_shard(registry_root, "card_int_id")[0].unlink()
    with pytest.raises(ValueError, match="allocation coverage"):
        load_registry(registry_root)


def test_shards_without_index_cannot_bootstrap_allocation(registry_root: Path) -> None:
    (registry_root / "ids/index.yaml").unlink()
    with pytest.raises(ValueError, match="allocation cursors"):
        load(registry_root)


def test_build_loader_requires_index_but_allocator_can_bootstrap(
    tmp_path: Path,
) -> None:
    assert load(tmp_path) == (Index(), {})
    with pytest.raises(ValueError, match="existing registry index"):
        load_registry(tmp_path)


def test_index_symlink_rejected(registry_root: Path, tmp_path: Path) -> None:
    path = registry_root / "ids/index.yaml"
    moved = tmp_path / "index-copy"
    path.rename(moved)
    path.symlink_to(moved)
    with pytest.raises(ValueError, match="Unsafe registry"):
        load_registry(registry_root)


def test_official_snapshot_preserves_all_allocations_and_nine_corrections(
    official_snapshot: RegistrySnapshot,
) -> None:
    root = Path(__file__).resolve().parents[2] / "authored"
    paths = [p for area in ("ids", "registry") for p in (root / area).rglob("*.yaml")]
    before = {p: hashlib.sha256(p.read_bytes()).digest() for p in paths}
    snapshot = official_snapshot
    assert (
        len([r for r in snapshot.records.values() if r.kind == "card_int_id"]) == 14789
    )
    assert snapshot.files.index().next_int_id == {"jp": 27370, "en": 67421}
    corrections = [
        r for r in snapshot.records.values() if isinstance(r.data, CorrectionData)
    ]
    assert len(corrections) == 9
    assert all(
        isinstance(r.data, CorrectionData) and r.data.state == "active"
        for r in corrections
    )
    for record in corrections:
        assert record.entry().data == record.data.model_dump(mode="json")
    assert before == {p: hashlib.sha256(p.read_bytes()).digest() for p in paths}


@pytest.mark.parametrize("is_index", [False, True])
@pytest.mark.parametrize("damage", ["missing_format", "bool_format", "missing_kind"])
def test_explicit_wire_envelopes(
    registry_root: Path, is_index: bool, damage: str
) -> None:

    path = (
        registry_root / "ids/index.yaml"
        if is_index
        else kind_shard(registry_root, "card")[0]
    )
    raw = read_yaml(path)
    assert isinstance(raw, dict)
    if damage == "bool_format":
        raw["authored_format"] = True
    else:
        raw.pop("authored_format" if damage == "missing_format" else "kind")
    with path.open("w") as stream:
        yaml_parser().dump(raw, stream)
    with pytest.raises(ValueError, match="envelope fields or format"):
        load_registry(registry_root)


def test_duplicate_allocations_cannot_hide_in_another_shard(
    registry_root: Path,
) -> None:
    path, _ = kind_shard(registry_root, "card_int_id")
    (path.parent / "999.yaml").write_bytes(path.read_bytes())
    with pytest.raises(ValueError, match="Duplicate record"):
        load_registry(registry_root)


@pytest.mark.parametrize(
    ("is_index", "field"),
    [(False, "decisions"), (False, "default_decision_id"), (True, "includes")],
)
def test_removed_envelope_fields_are_rejected(
    registry_root: Path, is_index: bool, field: str
) -> None:
    path = (
        registry_root / "ids/index.yaml"
        if is_index
        else kind_shard(registry_root, "card")[0]
    )
    raw = read_yaml(path)
    assert isinstance(raw, dict)
    raw[field] = []
    with path.open("w") as stream:
        yaml_parser().dump(raw, stream)
    with pytest.raises(ValueError, match="envelope fields or format"):
        load_registry(registry_root)
