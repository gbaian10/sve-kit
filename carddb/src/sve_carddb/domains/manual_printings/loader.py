"""Validate the complete manual-printing input before any build projection."""

from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError

from sve_carddb.core.authored import shards as authored_shards
from sve_carddb.core.json import digest
from sve_carddb.domains.manual_printings.records import (
    AREA,
    ManualEntry,
    ManualPrintingData,
    Shard,
)
from sve_carddb.domains.registry.allocation import (
    UNLISTED_RANGES,
    cursors,
    unlisted_range,
)

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.domains.registry.snapshot import RegistrySnapshot


@dataclass(frozen=True)
class LoadedShard:
    path: str
    content_hash: str
    content: bytes
    records: tuple[ManualEntry, ...]


@dataclass(frozen=True)
class ManualSnapshot:
    shards: tuple[LoadedShard, ...]
    records: Mapping[str, ManualEntry]

    def printings(self) -> tuple[ManualPrintingData, ...]:
        """Return every manual printing in a stable region/key order."""
        return tuple(
            sorted(
                (entry.data for entry in self.records.values()),
                key=lambda data: (data.region, data.key),
            )
        )

    def printing_ids(self) -> frozenset[str]:
        """Permanent printing IDs; the caller unions them with registry printings."""
        return frozenset(entry.data.id for entry in self.records.values())

    def configuration(self) -> list[JsonValue]:
        """Pin each shard by path and canonical hash, without card names."""
        return [
            {"path": shard.path, "sha256": shard.content_hash} for shard in self.shards
        ]


def load_manual_printings(
    root: Path,
    *,
    next_unlisted_int_id: Mapping[str, int],
    registry: RegistrySnapshot | None = None,
) -> ManualSnapshot:
    """Read every manual shard; a missing area is an empty, still-validated input."""
    loaded: list[LoadedShard] = []
    records: dict[str, ManualEntry] = {}
    for path, _, content in authored_shards(root, (AREA,), optional=(AREA,)):
        shard = _shard(content, path)
        owner = path.split("/")[2]
        for entry in shard.records:
            if entry.owner != owner:
                raise ValueError("Manual printing owner disagrees with its shard path")
            if entry.record_key in records:
                raise ValueError(f"Duplicate manual printing: {entry.record_key}")
            records[entry.record_key] = entry
        loaded.append(LoadedShard(path, digest(content), content, shard.records))
    snapshot = ManualSnapshot(tuple(loaded), MappingProxyType(records))
    _validate(snapshot, next_unlisted_int_id, registry)
    return snapshot


def _shard(content: bytes, path: str) -> Shard:
    try:
        return Shard.model_validate_json(content)
    except ValidationError as error:
        # Pydantic's default text repeats imported values; keep only the location.
        details = "; ".join(
            ".".join(map(str, issue["loc"])) + ":" + issue["type"]
            for issue in error.errors(include_input=False, include_context=False)
        )
        raise ValueError(f"Invalid manual printing shard {path}: {details}") from None


def _validate(
    snapshot: ManualSnapshot,
    next_unlisted_int_id: Mapping[str, int],
    registry: RegistrySnapshot | None,
) -> None:
    printings = snapshot.printings()
    keys = [data.key for data in printings]
    if len(keys) != len(set(keys)):
        raise ValueError("Manual printing keys must be unique")
    allocated: dict[str, list[int]] = {}
    seen: set[int] = set()
    for data in printings:
        if data.int_id not in unlisted_range(data.region) or data.int_id in seen:
            raise ValueError("Invalid, out-of-range or reused unlisted int_id")
        seen.add(data.int_id)
        allocated.setdefault(data.region, []).append(data.int_id)
    if dict(next_unlisted_int_id) != cursors(allocated, UNLISTED_RANGES):
        raise ValueError("Unlisted allocation high-water mark mismatch")
    if registry is not None:
        permanent = {
            getattr(record.data, "id", None) for record in registry.records.values()
        }
        for data in printings:
            if {data.id, data.card_id, data.face_id} & permanent:
                raise ValueError("Manual printing ID collides with the registry")
