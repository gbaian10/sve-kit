"""Validate the complete indexed current entry before returning any results."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError

from sve_carddb.core.authored import check_path
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import RecordData
from sve_carddb.domains.digital.art_matches.models import Index, Record, Shard
from sve_carddb.domains.registry.storage import read_yaml

if TYPE_CHECKING:
    from pathlib import Path


@dataclass(frozen=True)
class Snapshot:
    index: Index
    index_hash: str
    shards: tuple[tuple[str, bytes, Shard], ...]

    def records(self) -> tuple[Record, ...]:
        """Return the complete checked current result set."""
        return tuple(r for _, _, shard in self.shards for r in shard.records)

    def configuration(self) -> dict[str, JsonValue]:
        """Pin the recipe and exact authored inputs, without local paths."""
        return {
            "digital_art_authored": {
                "index_hash": self.index_hash,
                "recipe": self.index.recipe,
                "opencv_version": self.index.opencv_version,
                "shards": [
                    {"path": path, "sha256": digest(raw)}
                    for path, raw, _ in self.shards
                ],
            }
        }


def _model[T: RecordData](model: type[T], value: JsonValue) -> T:
    try:
        return model.model_validate_json(canonical(value))
    except ValidationError:
        raise ValueError("Invalid art match fields") from None


def load(root: Path) -> Snapshot | None:
    """An absent entry is unknown; a malformed or incomplete entry fails closed."""
    directory = root / "digital/art-matches"
    if not directory.exists() and not directory.is_symlink():
        return None
    index_path = directory / "index.yaml"
    check_path(root, index_path)
    index = _model(Index, read_yaml(index_path))
    files = tuple(directory.rglob("*"))
    for present_path in files:
        check_path(root, present_path)
    present = {
        p.relative_to(root).as_posix()
        for p in files
        if not p.is_dir() and p != index_path
    }
    if present != set(index.includes):
        raise ValueError("Art match indexed file closure differs from disk")
    shards = []
    subjects = set()
    for path, expected in sorted(index.includes.items()):
        if (
            re.fullmatch(r"digital/art-matches/[A-Za-z0-9_-]+/[0-9]{3,}\.yaml", path)
            is None
        ):
            raise ValueError("Unsafe art match include")
        file = root / path
        check_path(root, file)
        raw = file.read_bytes()
        value = read_yaml(file)
        if digest(canonical(value)) != expected:
            raise ValueError("Art match shard hash mismatch")
        shard = _model(Shard, value)
        for record in shard.records:
            key = record.art_id, record.game
            if key in subjects:
                raise ValueError("Duplicate art match subject")
            subjects.add(key)
        shards.append((path, raw, shard))
    return Snapshot(index, digest(index_path.read_bytes()), tuple(shards))
