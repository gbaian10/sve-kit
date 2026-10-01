"""Complete immutable adoption inventory and confirmed historical chain checks."""

import re
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- compare authored bytes to an immutable Git object
from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError

from sve_carddb.products.evidence import resolve_evidence
from sve_carddb.registry.records import FaceData, PrintingData, RecordData
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.wording_adoptions.models import (
    AdoptionRecord,
    Decision,
    Index,
    Mechanical,
    PreviousAdoption,
    Shard,
)

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.products.evidence import CheckedSource
    from sve_carddb.products.models import Evidence
    from sve_carddb.registry.snapshot import RegistrySnapshot

INDEX_PATH = "wording-adoptions/index.yaml"
_PATH = re.compile(r"wording-adoptions/(jp|en)/([0-9]{3,})\.yaml")


@dataclass(frozen=True)
class AdoptionFile:
    path: str
    content_hash: str
    exact_bytes: bytes
    envelope: Shard


@dataclass(frozen=True)
class AdoptionSnapshot:
    authored_revision: str
    index_hash: str
    index_bytes: bytes
    shards: tuple[AdoptionFile, ...]
    records: Mapping[str, AdoptionRecord]
    decisions: Mapping[str, Decision]
    record_decisions: Mapping[str, str]
    closure: Mapping[Evidence, CheckedSource]


def _file(root: Path, relative: str) -> Path:
    path = root / relative
    if not path.is_relative_to(root):
        raise ValueError("Unsafe adoption input path")
    for item in (path, *path.parents):
        if item.is_symlink():
            raise ValueError("Adoption input symlinks are forbidden")
        if item == root:
            break
    if not path.is_file():
        raise ValueError("Missing adoption input file")
    return path


def _model[T: RecordData](model: type[T], raw: JsonValue) -> T:
    try:
        return model.model_validate_json(canonical(raw))
    except ValidationError:
        raise ValueError("Invalid wording adoption fields") from None


def _inventory(root: Path, includes: set[str]) -> None:
    sequences: set[tuple[str, int]] = set()
    for relative in includes:
        match = _PATH.fullmatch(relative)
        if match is None or int(match[2]) < 1:
            raise ValueError("Unsafe adoption shard path")
        sequence = match[1], int(match[2])
        if sequence in sequences:
            raise ValueError("Duplicate regional adoption shard sequence")
        sequences.add(sequence)
    actual = set()
    for path in (root / "wording-adoptions").rglob("*"):
        if path.is_symlink():
            raise ValueError("Adoption inventory symlinks are forbidden")
        if path.suffix.lower() in {".yaml", ".yml"} and path != root / INDEX_PATH:
            actual.add(path.relative_to(root).as_posix())
    if actual != includes:
        raise ValueError("Adoption indexed file closure differs from disk")


def _immutable(root: Path, path: str, revision: str, content: bytes) -> None:
    executable = shutil.which("git")
    if executable is None:
        raise ValueError("Git is required for immutable adoption inputs")
    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed executable and validated Git object path, never a shell
        [executable, "-C", str(root.parent), "show", revision + ":authored/" + path],
        capture_output=True,
        check=False,
    )
    if result.returncode != 0 or result.stdout != content:
        raise ValueError("Adoption bytes differ from the pinned authored revision")


def _decision(shard: Shard) -> Decision:
    decision = shard.decisions[0]
    members = tuple(
        (record.record_key, digest(canonical(record.model_dump(mode="json"))))
        for record in shard.records
    )
    checksum = digest(canonical([[key, value] for key, value in members]))
    if (
        decision.members != members
        or decision.membership_hash != checksum
        or decision.id != "d:" + checksum.removeprefix("sha256:")
        or shard.default_decision_id != decision.id
        or decision.sample_ids != tuple(key for key, _ in members)
    ):
        raise ValueError("Confirmed adoption must cover every exact record member")
    reviews = {
        canonical(r.data.review.rule_set.model_dump(mode="json"))
        if r.data.review.rule_set
        else None
        for r in shard.records
    }
    modes = {r.data.review.mode for r in shard.records}
    if len(modes) != 1 or len(reviews) != 1:
        raise ValueError(
            "Human and policy reviews or distinct rule sets cannot share a shard"
        )
    if "approved_rules" in modes and "政策核可" not in decision.note:
        raise ValueError("Policy review must explicitly identify policy approval")
    return decision


def _shard(root: Path, path: str, expected: str, revision: str) -> AdoptionFile:
    source = _file(root, path)
    raw = read_yaml(source)
    if digest(canonical(raw)) != expected:
        raise ValueError("Modified immutable adoption shard")
    shard = _model(Shard, raw)
    keys = tuple(r.record_key for r in shard.records)
    if keys != tuple(sorted(set(keys))):
        raise ValueError("Adoption records must be sorted and unique")
    if any(r.filing_key != Path(path).parts[1] for r in shard.records):
        raise ValueError("Adoption record region disagrees with shard path")
    _decision(shard)
    content = source.read_bytes()
    _immutable(root, path, revision, content)
    return AdoptionFile(path, expected, content, shard)


def _references(
    records: dict[str, AdoptionRecord],
    decisions: dict[str, str],
    registry: RegistrySnapshot,
) -> None:
    faces = {
        r.data.id: r.data
        for r in registry.records.values()
        if isinstance(r.data, FaceData)
    }
    printings = {
        r.data.id: r.data
        for r in registry.records.values()
        if isinstance(r.data, PrintingData)
    }
    chains: dict[tuple[str, str], list[AdoptionRecord]] = {}
    for record in records.values():
        data = record.data
        face = faces.get(data.face_id)
        if face is None:
            raise ValueError("Adoption references an unregistered face")
        observations = data.observations
        if isinstance(data.previous, Mechanical):
            observations += data.previous.observations
        for observation in observations:
            printing = printings.get(observation.printing_id)
            if (
                printing is None
                or printing.card_id != face.card_id
                or printing.region != data.region
                or not any(
                    m.face_id == data.face_id
                    and m.source_index == observation.source_index
                    for m in printing.source_face_map
                )
            ):
                raise ValueError(
                    "Adoption observation printing/face/region mapping mismatch"
                )
        chains.setdefault((data.face_id, data.region), []).append(record)
    for chain in chains.values():
        _chain(chain, decisions)


def _chain(chain: list[AdoptionRecord], decisions: dict[str, str]) -> None:
    previous: AdoptionRecord | None = None
    for expected, record in enumerate(
        sorted(chain, key=lambda r: r.data.adoption_no), 1
    ):
        if record.data.adoption_no != expected:
            raise ValueError("Adoption chain must have consecutive numbers")
        predecessor = record.data.previous
        if expected > 1:
            if (
                previous is None
                or not isinstance(predecessor, PreviousAdoption)
                or (
                    predecessor.record_key != previous.record_key
                    or predecessor.record_hash
                    != digest(canonical(previous.model_dump(mode="json")))
                    or predecessor.decision_id != decisions[previous.record_key]
                )
            ):
                raise ValueError(
                    "Adoption predecessor must match the exact prior confirmed record"
                )
            order = record.data.previous_order
            if (
                order is not None
                and order.review_receipt is not None
                and (
                    order.review_receipt.before_observation_keys
                    != (previous.data.selected_observation_key,)
                    or order.review_receipt.after_observation_keys
                    != (record.data.selected_observation_key,)
                )
            ):
                raise ValueError(
                    "Previous-order answer disagrees with both selected observations"
                )
        previous = record


def load_adoptions(
    root: Path,
    *,
    authored_revision: str,
    registry: RegistrySnapshot,
    stores: Mapping[str, Path],
) -> AdoptionSnapshot:
    """Validate both regions and frozen evidence before selecting or consuming records."""
    if re.fullmatch(r"[0-9a-f]{40}", authored_revision) is None:
        raise ValueError("Adoption authored revision must be a complete Git SHA")
    path = _file(root, INDEX_PATH)
    raw = read_yaml(path)
    index = _model(Index, raw)
    _inventory(root, set(index.includes))
    content = path.read_bytes()
    _immutable(root, INDEX_PATH, authored_revision, content)
    shards = tuple(
        _shard(root, name, checksum, authored_revision)
        for name, checksum in sorted(index.includes.items())
    )
    records: dict[str, AdoptionRecord] = {}
    decisions: dict[str, Decision] = {}
    record_decisions: dict[str, str] = {}
    for shard in shards:
        decision = shard.envelope.decisions[0]
        if decision.id in decisions:
            raise ValueError("Duplicate adoption decision")
        decisions[decision.id] = decision
        for record in shard.envelope.records:
            if record.record_key in records:
                raise ValueError("Duplicate adoption record key")
            records[record.record_key] = record
            record_decisions[record.record_key] = decision.id
    _references(records, record_decisions, registry)
    closure = resolve_evidence(
        tuple(ref for record in records.values() for ref in record.evidence), stores
    )
    return AdoptionSnapshot(
        authored_revision,
        digest(canonical(raw)),
        content,
        shards,
        MappingProxyType(records),
        MappingProxyType(decisions),
        MappingProxyType(record_decisions),
        MappingProxyType(dict(closure)),
    )
