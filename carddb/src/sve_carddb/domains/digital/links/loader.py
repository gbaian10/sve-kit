"""Validate the complete entry before returning current relations."""

import re
from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import RecordData
from sve_carddb.domains.digital.links.models import Index, Record, Shard
from sve_carddb.domains.registry.storage import read_yaml

if TYPE_CHECKING:
    from pathlib import Path


def link_id(record: Record) -> str:
    """Content-address the subject independently of relation changes."""
    return (
        "dl:"
        + digest(
            canonical(["digital-link-v1", record.subject.model_dump(mode="json")])
        )[7:]
    )


def decision_id(record: Record) -> str:
    """Each authored relation is one record-level decision in the build DB."""
    return (
        "d:"
        + digest(
            canonical(["digital-link-decision-v1", record.model_dump(mode="json")])
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

    def records(self) -> tuple[Record, ...]:
        """One current relation per subject; Git keeps earlier versions."""
        return tuple(record for shard in self.envelopes() for record in shard.records)

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


def load_links(  # ruff: ignore[complex-structure] -- whole-entry validation precedes any relation
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
        for record in _model(Shard, read_yaml(file)).records:
            _evidence(record)
        shards.append((name, file.read_bytes(), content))
    snapshot = Snapshot(index_path.read_bytes(), tuple(shards))
    subjects = [link_id(record) for record in snapshot.records()]
    if len(subjects) != len(set(subjects)):
        raise ValueError("Duplicate digital-link record")
    return snapshot


def _evidence(record: Record) -> None:
    value = record.value
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
    subject = record.subject
    if subject.face_id is not None and (
        any(name.face_id != subject.face_id for name in value.sve_names)
        or phases != {subject.digital_phase}
    ):
        raise ValueError("Digital-link evidence differs from declared faces")
