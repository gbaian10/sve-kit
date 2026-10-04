"""Validate complete immutable histories before selecting terminal relations."""

import re
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

from pydantic import JsonValue, ValidationError

from sve_carddb.catalog.adoption_loader import ordered
from sve_carddb.digital_links.models import Index, Record, Shard
from sve_carddb.registry.records import RecordData
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import canonical, digest


def record_hash(record: Record) -> str:
    """Bind the complete relation, sources and review background."""
    return digest(canonical(record.model_dump(mode="json")))


def subject_key(record: Record) -> bytes:
    """Keep the full subject stable across adoption revisions."""
    return canonical([record.kind, record.data.subject.model_dump(mode="json")])


def key(record: Record) -> str:
    """Use the contract canonical key, without concatenated identity guesses."""
    return canonical(
        [
            record.kind,
            record.data.subject.model_dump(mode="json"),
            record.data.adoption_no,
        ]
    ).decode()


def link_id(record: Record) -> str:
    """Content-address the subject independently of relation revisions."""
    return (
        "dl:"
        + digest(
            canonical(["digital-link-v1", record.data.subject.model_dump(mode="json")])
        )[7:]
    )


@dataclass(frozen=True)
class Snapshot:
    index: bytes
    shards: tuple[tuple[str, bytes, bytes], ...]

    def envelopes(self) -> tuple[Shard, ...]:
        """Return detached envelopes so callers cannot mutate the checked snapshot."""
        return tuple(
            Shard.model_validate_json(content) for _, _, content in self.shards
        )

    def records(self) -> tuple[tuple[Record, str], ...]:
        """Retain all history for replay and audit."""
        return tuple(
            (record, shard.default_decision_id)
            for shard in self.envelopes()
            for record in shard.records
        )

    def effective(self) -> tuple[tuple[Record, str], ...]:
        """Select terminal revisions only; stale or withdrawn values never resurrect."""
        latest: dict[bytes, tuple[Record, str]] = {}
        for record, decision in self.records():
            subject = subject_key(record)
            if (
                subject not in latest
                or latest[subject][0].data.adoption_no < record.data.adoption_no
            ):
                latest[subject] = record, decision
        return tuple(latest[subject] for subject in sorted(latest))

    def pins(self) -> dict[str, JsonValue]:
        """Pin original bytes separately from canonical shard content."""
        return {
            "index_hash": digest(self.index),
            "shards": [
                {
                    "path": path,
                    "exact_hash": digest(exact),
                    "canonical_hash": digest(content),
                }
                for path, exact, content in self.shards
            ],
        }


def _model[T: RecordData](model: type[T], raw: JsonValue) -> T:
    try:
        return model.model_validate_json(canonical(raw))
    except ValidationError:
        # Source-bearing inputs must not be copied into Pydantic error reports.
        raise ValueError("Invalid digital-link fields") from None


def _safe(path: Path) -> None:
    if any(part.is_symlink() for part in (path, *path.parents)):
        raise ValueError("Symlink digital-link input")
    if not path.is_file():
        raise ValueError("Missing digital-link input")


def load_links(  # ruff: ignore[complex-structure] -- whole-entry validation precedes terminal projection
    root: Path,
) -> Snapshot:
    """Validate the entire entry before projecting any public relations."""
    directory = root.absolute() / "digital-links"
    index_path = directory / "index.yaml"
    _safe(index_path)
    index = _model(Index, read_yaml(index_path))
    sequences: dict[str, list[int]] = defaultdict(list)
    for name in index.includes:
        if name.startswith("digital-links/coverage/"):
            raise ValueError("Digital coverage adoption is not supported")
        match = re.fullmatch(
            r"digital-links/links/([A-Za-z0-9_-]+)/([0-9]{3,})\.yaml", name
        )
        if match is None:
            raise ValueError("Unsafe digital-link include")
        sequences[match[1]].append(int(match[2]))
    present = set()
    for file in directory.rglob("*"):
        if file.is_symlink():
            raise ValueError("Symlink digital-link input")
        if file != index_path and not file.is_dir():
            present.add(file.relative_to(root.absolute()).as_posix())
    if present != set(index.includes):
        raise ValueError("Digital-link indexed file closure differs from disk")
    if any(
        sorted(numbers) != list(range(1, len(numbers) + 1))
        for numbers in sequences.values()
    ):
        raise ValueError("Digital-link shard sequence gap")
    shards = []
    for name, checksum in sorted(index.includes.items()):
        file = root / name
        _safe(file)
        content = canonical(read_yaml(file))
        if digest(content) != checksum:
            raise ValueError("Digital-link shard hash mismatch")
        shard = _model(Shard, read_yaml(file))
        _envelope(shard, Path(name).parts[2])
        shards.append((name, file.read_bytes(), content))
    snapshot = Snapshot(index_path.read_bytes(), tuple(shards))
    _history(snapshot)
    return snapshot


def _envelope(  # ruff: ignore[complex-structure] -- membership and actual receipt are independently checked
    shard: Shard, filing: str
) -> None:
    keys = tuple(record.record_key for record in shard.records)
    if keys != tuple(sorted(set(keys))):
        raise ValueError("Digital-link members must be sorted and unique")
    ordered(shard.review_context.source_batches)
    for record in shard.records:
        if record.record_key != key(record) or record.filing_key != filing:
            raise ValueError("Digital-link record key or filing mismatch")
        if record.data.review_context_hash != digest(
            canonical(shard.review_context.model_dump(mode="json"))
        ):
            raise ValueError("Digital-link review context hash mismatch")
        _evidence(record, shard)
    decision = shard.decisions[0]
    members = tuple(
        (record.record_key, record_hash(record)) for record in shard.records
    )
    checksum = digest(canonical([list[JsonValue](member) for member in members]))
    if decision.members != members:
        raise ValueError("Digital-link decision members mismatch")
    if decision.membership_hash != checksum:
        raise ValueError("Digital-link membership hash mismatch")
    if decision.id != "d:" + checksum[7:]:
        raise ValueError("Digital-link decision ID mismatch")
    if shard.default_decision_id != decision.id:
        raise ValueError("Digital-link default decision mismatch")
    if (
        not decision.sample_ids
        or decision.sample_ids != tuple(sorted(set(decision.sample_ids)))
        or not set(decision.sample_ids) <= set(keys)
    ):
        raise ValueError("Digital-link decision requires actual checked members")
    if decision.state == "confirmed" and decision.sample_ids != keys:
        raise ValueError("Confirmed digital-link decision must check every member")


def _evidence(record: Record, shard: Shard) -> None:
    ordered(record.evidence)
    value = record.data.value
    if value is None:
        if record.evidence:
            raise ValueError("Withdrawn digital link must have empty evidence")
        return
    sve_keys = [
        (
            name.printing_id,
            name.face_id,
            canonical(name.name_ref.model_dump(mode="json")),
        )
        for name in value.sve_names
    ]
    if sve_keys != sorted(set(sve_keys)):
        raise ValueError("SVE name evidence must be sorted and unique")
    digital_keys = [(name.phase, name.lang) for name in value.digital_names]
    if digital_keys != sorted(set(digital_keys)):
        raise ValueError("Digital name phases and languages must be sorted and unique")
    phases = {name.phase for name in value.digital_names}
    if any((phase, "ja") not in digital_keys for phase in phases):
        raise ValueError("Digital name evidence requires Japanese for every phase")
    subject = record.data.subject
    if subject.face_id is not None and (
        any(name.face_id != subject.face_id for name in value.sve_names)
        or phases != {subject.digital_phase}
    ):
        raise ValueError("Digital-link evidence differs from declared faces")
    expected = {
        canonical({"source_ref": name.name_ref.model_dump(mode="json"), "role": role})
        for names, role in (
            (value.sve_names, "sve_name"),
            (value.digital_names, "digital_name"),
        )
        for name in names
    }
    if {canonical(e.model_dump(mode="json")) for e in record.evidence} != expected:
        raise ValueError("Digital-link evidence set differs from name references")
    batches = {batch.batch_id for batch in shard.review_context.source_batches}
    if any(e.source_ref.batch_id not in batches for e in record.evidence):
        raise ValueError("Digital-link evidence batch absent from review context")


def _history(snapshot: Snapshot) -> None:
    seen = set()
    chains: dict[bytes, list[tuple[Record, str]]] = defaultdict(list)
    for record, decision in snapshot.records():
        if record.record_key in seen:
            raise ValueError("Duplicate digital-link record")
        seen.add(record.record_key)
        chains[subject_key(record)].append((record, decision))
    for chain in chains.values():
        chain.sort(key=lambda item: item[0].data.adoption_no)
        previous = None
        for number, (record, decision) in enumerate(chain, 1):
            if record.data.adoption_no != number:
                raise ValueError("Digital-link adoption sequence gap or fork")
            predecessor = record.data.predecessor
            if previous is None:
                if predecessor is not None or record.data.value is None:
                    raise ValueError(
                        "Initial digital-link adoption must have value and no predecessor"
                    )
            elif predecessor is None or (
                predecessor.record_key,
                predecessor.record_hash,
                predecessor.decision_id,
            ) != (previous[0].record_key, record_hash(previous[0]), previous[1]):
                raise ValueError("Digital-link predecessor mismatch")
            previous = record, decision
