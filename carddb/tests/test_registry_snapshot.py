"""Registry envelope preservation and independent build-input counterexamples."""

import hashlib
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.registry.inputs import digest
from sve_carddb.registry.records import CorrectionData, EnglishPrintingData
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.registry.storage import (
    Index,
    Shard,
    encode,
    load,
    read_yaml,
    yaml_parser,
)

from .registry_snapshot_fixtures import decision_shard, edit_record, rewrite
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
            assert record.decision_id == envelope.default_decision_id
        for decision in envelope.decisions:
            assert snapshot.decisions[decision.id].model_dump(
                mode="json"
            ) == decision.model_dump(mode="json")
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
    snapshot.files.index().includes.clear()
    snapshot.files.shards[0].envelope().records.clear()
    assert record.entry().data
    assert snapshot.files.index().includes
    assert snapshot.files.shards[0].envelope().records
    with pytest.raises(TypeError):
        snapshot.records["changed"] = record  # type: ignore[index]  # exercise runtime immutability
    with pytest.raises(ValidationError, match="frozen"):
        record.data.extra = "changed"  # type: ignore[attr-defined]  # exercise runtime immutability


@pytest.mark.parametrize(
    "damage",
    [
        "members",
        "semantic_hash",
        "membership_hash",
        "id",
        "pointer",
        "checked_missing",
        "reviewer",
        "reviewed_at",
    ],
)
def test_each_confirmed_envelope_invariant(registry_root: Path, damage: str) -> None:
    path, shard = decision_shard(registry_root)
    decision = shard.decisions[0]
    if damage == "members":
        decision.members = []
    elif damage == "semantic_hash":
        decision.members[0] = (decision.members[0][0], "sha256:" + "0" * 64)
    elif damage == "membership_hash":
        decision.membership_hash = "sha256:" + "0" * 64
        decision.id = "d:" + "0" * 64
        shard.default_decision_id = decision.id
    elif damage == "id":
        decision.id = "d:" + "0" * 64
        shard.default_decision_id = decision.id
    elif damage == "pointer":
        shard.default_decision_id = "d:" + "0" * 64
    elif damage == "checked_missing":
        decision.sample_ids = []
    elif damage == "reviewer":
        decision.reviewed_by = None
    else:
        decision.reviewed_at = None
    rewrite(registry_root, path, shard)
    with pytest.raises(ValueError, match=r"Decision membership|Confirmed batch"):
        load_registry(registry_root)


@pytest.mark.parametrize("duplicate", [False, True])
def test_proposed_checked_members_are_unique_subset(
    registry_root: Path, duplicate: bool
) -> None:
    path, shard = decision_shard(registry_root, proposed=True)
    decision = shard.decisions[0]
    decision.sample_ids = (
        [decision.members[0][0]] * 2 if duplicate else ["not-a-member"]
    )
    rewrite(registry_root, path, shard)
    with pytest.raises(ValueError, match="unique subset"):
        load_registry(registry_root)


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


@pytest.mark.parametrize("kind", ["missing", "modified", "unindexed"])
def test_indexed_closure_is_mandatory(registry_root: Path, kind: str) -> None:
    path, _ = decision_shard(registry_root)
    if kind == "missing":
        path.unlink()
    elif kind == "modified":
        path.write_text(path.read_text().replace("reviewer", "other-reviewer"))
    else:
        (path.parent / "999.yaml").write_text("not: a valid shard\n")
    with pytest.raises(ValueError, match=r"closure|immutable shard"):
        load_registry(registry_root)


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


def test_allocations_cannot_inherit_decisions(registry_root: Path) -> None:
    path, shard = decision_shard(registry_root)
    _, entries = load(registry_root)
    allocation = next(r for r in entries.values() if r.kind == "card_int_id")
    index = Index.model_validate(read_yaml(registry_root / "ids/index.yaml"))
    for name in index.includes:
        if name.startswith("ids/"):
            source = registry_root / name
            allocations = Shard.model_validate(read_yaml(source))
            if allocation in allocations.records:
                allocations.records.remove(allocation)
                rewrite(registry_root, source, allocations)
                break
    shard.records.append(allocation)
    rewrite(registry_root, path, shard, resign=True)
    with pytest.raises(ValueError, match="Allocations cannot share"):
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
        assert record.decision_id is not None
        assert snapshot.decisions[record.decision_id].state == "confirmed"
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
        else decision_shard(registry_root)[0]
    )
    raw = read_yaml(path)
    assert isinstance(raw, dict)
    if damage == "bool_format":
        raw["authored_format"] = True
    else:
        raw.pop("authored_format" if damage == "missing_format" else "kind")
    with path.open("w") as stream:
        yaml_parser().dump(raw, stream)
    if not is_index:
        index_path = registry_root / "ids/index.yaml"
        index = Index.model_validate(read_yaml(index_path))
        index.includes[path.relative_to(registry_root).as_posix()] = digest(raw)
        index_path.write_bytes(encode(index))
    with pytest.raises(ValueError, match="envelope fields or format"):
        load_registry(registry_root)


def test_duplicate_allocations_cannot_hide_in_another_shard(
    registry_root: Path,
) -> None:
    index_path = registry_root / "ids/index.yaml"
    index = Index.model_validate(read_yaml(index_path))
    name = next(name for name in index.includes if name.startswith("ids/"))
    extra = "ids/BP02/999.yaml"
    (registry_root / extra).write_bytes((registry_root / name).read_bytes())
    index.includes[extra] = index.includes[name]
    index_path.write_bytes(encode(index))
    with pytest.raises(ValueError, match="Duplicate record"):
        load_registry(registry_root)


@pytest.mark.parametrize("field", ["authored_at", "reviewed_at", "authored_by"])
def test_typed_decision_metadata(registry_root: Path, field: str) -> None:
    path, shard = decision_shard(registry_root)
    decision = shard.decisions[0]
    damaged = decision.model_dump()
    damaged[field] = "not-an-instant" if field.endswith("_at") else ""
    shard.decisions[0] = type(decision).model_validate(damaged)
    rewrite(registry_root, path, shard)
    with pytest.raises(ValueError, match="Invalid registry decision fields"):
        load_registry(registry_root)
