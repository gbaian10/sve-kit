"""Validate the complete adopted glossary history before projecting terminal choices."""

import re
from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.catalog.adoption_loader import ordered
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, parse
from sve_carddb.translations.models import (
    ChoiceRecord,
    Index,
    Record,
    Shard,
    TermRecord,
    VocabularyRecord,
)

if TYPE_CHECKING:
    from pathlib import Path


def record_hash(record: Record) -> str:
    """Hash the full adopted payload including its evidence."""
    return digest(canonical(record.model_dump(mode="json")))


def key(record: Record) -> str:
    """Use explicit primary keys rather than names, translated text or file order."""
    if isinstance(record, TermRecord):
        fields: list[JsonValue] = [record.kind, record.data.id]
    elif isinstance(record, ChoiceRecord):
        fields = [record.kind, record.data.term_id, record.data.lang]
    else:
        fields = [
            record.kind,
            record.data.vocabulary_kind,
            record.data.vocabulary_code,
            record.data.lang,
        ]
    if not isinstance(record, TermRecord):
        fields.append(record.data.adoption_no)
    return canonical(fields).decode()


@dataclass(frozen=True)
class Snapshot:
    index: bytes
    shards: tuple[tuple[str, bytes, bytes], ...]

    def envelopes(self) -> tuple[Shard, ...]:
        """Return detached models; callers cannot mutate the approved byte snapshot."""
        return tuple(
            Shard.model_validate_json(content) for _, _, content in self.shards
        )

    def records(self) -> tuple[tuple[Record, str], ...]:
        """Retain historical records together with exact batch decisions."""
        return tuple(
            (r, s.default_decision_id) for s in self.envelopes() for r in s.records
        )

    def effective(self) -> tuple[tuple[Record, str], ...]:
        """Choose terminal adopted revisions after complete chain validation."""
        latest: dict[str, tuple[Record, str]] = {}
        for record, decision in self.records():
            subject = (
                record.record_key
                if isinstance(record, TermRecord)
                else canonical(array(parse(record.record_key.encode()))[:-1]).decode()
            )
            previous = latest.get(subject)
            if previous is None or (
                not isinstance(record, TermRecord)
                and record.data.adoption_no > revision(previous[0])
            ):
                latest[subject] = record, decision
        return tuple(latest[name] for name in sorted(latest))

    def pins(self) -> dict[str, JsonValue]:
        """Distinguish exact YAML inputs from canonical membership hashes."""
        return {
            "index_hash": digest(self.index),
            "shards": [
                {
                    "path": p,
                    "exact_hash": digest(exact),
                    "canonical_hash": digest(content),
                }
                for p, exact, content in self.shards
            ],
        }


def revision(record: Record) -> int:
    """Immutable concepts have no adoption revision."""
    return 0 if isinstance(record, TermRecord) else record.data.adoption_no


def _safe(path: Path) -> None:
    if any(part.is_symlink() for part in (path, *path.parents)) or not path.is_file():
        raise ValueError("Missing or symlink translation input")


def load_glossary(root: Path) -> Snapshot:  # ruff: ignore[complex-structure] -- every filesystem closure guard precedes projection
    """Reject unsupported areas and inventories, rather than treating them as empty."""
    path = root / "translations/index.yaml"
    _safe(path)
    index = Index.model_validate_json(canonical(read_yaml(path)))
    if index.inventories:
        raise ValueError("Template inventories require the #52 loader")
    present = set()
    for file in path.parent.rglob("*"):
        if file.is_symlink():
            raise ValueError("Symlink translation input")
        if not file.is_dir() and file != path:
            present.add(file.relative_to(root).as_posix())
    if present != set(index.includes):
        raise ValueError("Translation indexed file closure differs from disk")
    sequences: dict[str, list[int]] = defaultdict(list)
    shards = []
    for name, checksum in sorted(index.includes.items()):
        match = re.fullmatch(
            r"translations/glossary/([A-Za-z0-9_-]+)/([0-9]{3,})\.yaml", name
        )
        if match is None:
            raise ValueError("Unsafe or unsupported translation include")
        sequences[match[1]].append(int(match[2]))
        file = root / name
        _safe(file)
        content = canonical(read_yaml(file))
        if digest(content) != checksum:
            raise ValueError("Translation shard hash mismatch")
        shard = Shard.model_validate_json(content)
        _envelope(shard, match[1])
        shards.append((name, file.read_bytes(), content))
    for numbers in sequences.values():
        if sorted(numbers) != list(range(1, len(numbers) + 1)):
            raise ValueError("Translation shard sequence gap")
    snapshot = Snapshot(path.read_bytes(), tuple(shards))
    _history(snapshot)
    return snapshot


def _envelope(shard: Shard, filing: str) -> None:
    decision = shard.decisions[0]
    keys = [r.record_key for r in shard.records]
    if keys != sorted(set(keys)):
        raise ValueError("Translation members must be sorted and unique")
    members = tuple((r.record_key, record_hash(r)) for r in shard.records)
    checksum = digest(canonical([[k, h] for k, h in members]))
    if (
        decision.members != members
        or decision.membership_hash != checksum
        or decision.id != "d:" + checksum.removeprefix("sha256:")
        or shard.default_decision_id != decision.id
    ):
        raise ValueError("Translation decision exact membership mismatch")
    if (
        not decision.sample_ids
        or tuple(sorted(set(decision.sample_ids))) != decision.sample_ids
        or not set(decision.sample_ids) <= set(keys)
    ):
        raise ValueError("Translation decision requires actual checked members")
    if decision.state == "confirmed" and decision.sample_ids != tuple(keys):
        raise ValueError("Confirmed translation must check every member")
    for record in shard.records:
        if (
            record.kind != decision.category
            or record.filing_key != filing
            or record.record_key != key(record)
        ):
            raise ValueError("Translation record kind/key/filing mismatch")
        ordered(record.evidence)
        if (
            isinstance(record, TermRecord)
            and record.data.id != "term:" + record.data.concept_key
        ):
            raise ValueError("Permanent term ID differs from concept key")


def _history(snapshot: Snapshot) -> None:  # ruff: ignore[complex-structure] -- immutable allocations and revision chains share one complete inventory
    seen: dict[str, tuple[Record, str]] = {}
    concepts = set()
    chains: dict[str, list[tuple[ChoiceRecord | VocabularyRecord, str]]] = defaultdict(
        list
    )
    for record, decision in snapshot.records():
        if record.record_key in seen:
            raise ValueError("Duplicate immutable translation record")
        seen[record.record_key] = record, decision
        if isinstance(record, TermRecord):
            if record.data.concept_key in concepts:
                raise ValueError("Concept key already allocated")
            concepts.add(record.data.concept_key)
        else:
            subject = canonical(array(parse(record.record_key.encode()))[:-1]).decode()
            chains[subject].append((record, decision))
    for chain in chains.values():
        chain.sort(key=lambda item: item[0].data.adoption_no)
        previous = None
        for number, (record, decision) in enumerate(chain, 1):
            if record.data.adoption_no != number:
                raise ValueError("Translation adoption sequence gap or fork")
            predecessor = record.data.predecessor
            if previous is None:
                if predecessor is not None:
                    raise ValueError("Initial choice must have no predecessor")
            elif predecessor is None or (
                predecessor.record_key,
                predecessor.record_hash,
                predecessor.decision_id,
            ) != (previous[0].record_key, record_hash(previous[0]), previous[1]):
                raise ValueError("Translation predecessor mismatch")
            previous = record, decision
    terms = {r.data.id for r, _ in snapshot.records() if isinstance(r, TermRecord)}
    if any(
        isinstance(r, ChoiceRecord) and r.data.term_id not in terms
        for r, _ in snapshot.records()
    ):
        raise ValueError("Glossary choice references an unadopted concept")
