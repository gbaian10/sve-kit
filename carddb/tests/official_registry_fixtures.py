"""One validated official registry per pytest run, including xdist workers."""

import fcntl
import hashlib
from copy import deepcopy
from types import MappingProxyType
from typing import TYPE_CHECKING

from pydantic import BaseModel

from sve_carddb.registry.snapshot import (
    RegistryRecord,
    RegistrySnapshot,
    _data,
    load_registry,
)
from sve_carddb.registry.storage import Entry, RegistryFiles

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path


def detached_entries(entries: tuple[Entry, ...]) -> list[Entry]:
    """The legacy API contains nested mutable lists and dictionaries."""
    return deepcopy(list(entries))


class CachedRecord(BaseModel):
    shard_path: str
    content: bytes

    def record(self) -> RegistryRecord:
        entry = Entry.model_validate_json(self.content)
        return RegistryRecord(
            entry.record_key,
            entry.kind,
            entry.owner,
            _data(entry),
            self.shard_path,
            self.content,
        )


class CachedRegistry(BaseModel):
    files: RegistryFiles
    records: tuple[CachedRecord, ...]

    def snapshot(self) -> RegistrySnapshot:
        records = [item.record() for item in self.records]
        return RegistrySnapshot(
            self.files,
            MappingProxyType({record.record_key: record for record in records}),
        )


def source_hashes(root: Path) -> Mapping[Path, bytes]:
    """Keep the first source load inside the no-write regression boundary."""
    return {
        path: hashlib.sha256(path.read_bytes()).digest()
        for area in ("ids", "registry")
        for path in (root / area).rglob("*.yaml")
    }


def load_shared_registry(root: Path, cache: Path) -> RegistrySnapshot:
    """Use only this run's private temp cache; each worker gets frozen values."""
    # A session fixture alone would load once per xdist worker.
    with cache.with_suffix(".lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if cache.exists():
            return CachedRegistry.model_validate_json(cache.read_bytes()).snapshot()
        before = source_hashes(root)
        snapshot = load_registry(root)
        assert before == source_hashes(root), "Registry load changed authored files"
        cached = CachedRegistry(
            files=snapshot.files,
            records=tuple(
                CachedRecord(
                    shard_path=record.shard_path,
                    content=record.content,
                )
                for record in snapshot.records.values()
            ),
        )
        cache.write_text(cached.model_dump_json(), encoding="utf-8")
        return snapshot
