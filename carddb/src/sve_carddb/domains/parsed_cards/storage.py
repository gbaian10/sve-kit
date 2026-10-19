"""Validate the whole reviewed input closure before exposing any source document."""

from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

from sve_carddb.core.json import digest
from sve_carddb.domains.parsed_cards.codec import decode, encode
from sve_carddb.domains.parsed_cards.filing import band as number_band
from sve_carddb.domains.parsed_cards.filing import shard_path
from sve_carddb.domains.parsed_cards.models import (
    MAX_BYTES,
    Card,
    Index,
    ParsedObservation,
    Shard,
    ShardPin,
)
from sve_carddb.domains.parsed_cards.serialization import combine, separate
from sve_carddb.domains.parsed_cards.wire import CardFile, EvidenceFile

if TYPE_CHECKING:
    from sve_carddb.core.regions import Region


def member(root: Path, name: str) -> Path:
    """Confine reads and writes to regular members of the selected input root."""
    relative = PurePosixPath(name)
    if relative.is_absolute() or ".." in relative.parts or str(relative) != name:
        raise ValueError("Unsafe parsed data member")
    path = root
    for part in relative.parts:
        path /= part
        if path.is_symlink():
            raise ValueError("Parsed data members cannot be symlinks")
    return path


@dataclass(frozen=True)
class Catalog:
    index: Index
    shards: tuple[Shard, ...]

    def observations(self) -> dict[str, ParsedObservation]:
        """Expose only validated complete source observations."""
        return {
            o.evidence.descriptor.id: o
            for s in self.shards
            for c in s.cards
            for o in c.observations
        }

    def validate_update(self, previous: Catalog) -> None:
        """A self-consistent new index cannot authorize deleting old source evidence."""
        current = self.observations()
        for version, old in previous.observations().items():
            new = current.get(version)
            if new is None or new.content_hash != old.content_hash:
                raise ValueError(
                    "Parsed update deletes or replaces a historical observation"
                )
            if not set(old.memberships) <= set(new.memberships):
                raise ValueError("Parsed update drops historical batch membership")
        old_batches = {b.id: b for b in previous.index.batches}
        if any(
            old_batches.get(b.id, b) != b for b in self.index.batches
        ) or not old_batches.keys() <= {b.id for b in self.index.batches}:
            raise ValueError("Parsed update changes a historical batch")
        old_filing = {
            (s.region, c.card_no): (s.family, s.band)
            for s in previous.shards
            for c in s.cards
        }
        if any(
            old_filing.get((s.region, c.card_no), (s.family, s.band))
            != (s.family, s.band)
            for s in self.shards
            for c in s.cards
        ):
            raise ValueError("Parsed update changes existing filing")


@dataclass
class _Closure:
    index: Index
    cards: set[tuple[Region, str]] = field(default_factory=set)
    versions: set[str] = field(default_factory=set)
    batches: dict[str, list[bool]] = field(default_factory=dict)
    current_urls: set[tuple[str, str]] = field(default_factory=set)

    def observation(self, value: ParsedObservation, region: Region) -> None:
        descriptor = value.evidence.descriptor
        if descriptor.id in self.versions:
            raise ValueError("Duplicate parsed source version")
        self.versions.add(descriptor.id)
        regions = {b.id: b.region for b in self.index.batches}
        for membership in value.memberships:
            if regions.get(membership.batch_id) != region:
                raise ValueError("Parsed source belongs to an unknown/foreign batch")
            self.batches.setdefault(membership.batch_id, []).append(membership.current)
            if membership.current:
                current_key = membership.batch_id, descriptor.url
                if current_key in self.current_urls:
                    raise ValueError("Multiple current versions of a parsed page")
                self.current_urls.add(current_key)

    def card(self, card: Card, shard: Shard, path: str) -> None:
        key = shard.region, card.card_no
        splits = {(s.region, s.family) for s in self.index.split_families}
        expected = shard_path(shard.region, shard.family, card.card_no, splits)
        band = (
            number_band(card.card_no, shard.region)
            if (shard.region, shard.family) in splits
            else None
        )
        if key in self.cards or path != expected or shard.band != band:
            raise ValueError("Duplicate or incorrectly filed parsed card")
        self.cards.add(key)
        for observation in card.observations:
            self.observation(observation, shard.region)

    def finish(self) -> None:
        for batch in self.index.batches:
            items = self.batches.get(batch.id, [])
            if (len(items), sum(items)) != (batch.versions, batch.current):
                raise ValueError("Parsed batch is incomplete")


def _read_shard(root: Path, pin: ShardPin) -> Shard:
    raw = member(root, pin.path).read_bytes()
    if len(raw) != pin.bytes or digest(raw) != pin.sha256:
        raise ValueError("Parsed shard size/hash mismatch")
    evidence_raw = member(root, pin.evidence_path).read_bytes()
    if (
        len(evidence_raw) != pin.evidence_bytes
        or digest(evidence_raw) != pin.evidence_sha256
    ):
        raise ValueError("Parsed evidence size/hash mismatch")
    shard = combine(decode(CardFile, raw), decode(EvidenceFile, evidence_raw))
    if (len(shard.cards), sum(len(c.observations) for c in shard.cards)) != (
        pin.cards,
        pin.observations,
    ):
        raise ValueError("Parsed shard member count mismatch")
    return shard


def load(root: Path) -> Catalog:
    """Read no raw source, manifest SQLite, network resource, or implicit fallback."""
    if not root.is_absolute() or root.is_symlink():
        raise ValueError("Parsed data root must be absolute and not a symlink")
    index_raw = member(root, "index.json").read_bytes()
    if len(index_raw) > MAX_BYTES:
        raise ValueError("Parsed index exceeds 1 MiB")
    index = decode(Index, index_raw)
    closure = _Closure(index)
    shards: list[Shard] = []
    for pin in index.shards:
        shard = _read_shard(root, pin)
        for card in shard.cards:
            closure.card(card, shard, pin.path)
        shards.append(shard)
    closure.finish()
    expected_paths = (
        {p.path for p in index.shards}
        | {p.evidence_path for p in index.shards}
        | {"index.json"}
    )
    actual_paths = {p.relative_to(root).as_posix() for p in root.rglob("*.json")}
    if expected_paths != actual_paths:
        raise ValueError("Parsed directory contains missing or unlisted JSON members")
    return Catalog(index, tuple(shards))


def write(root: Path, catalog: Catalog) -> Catalog:
    """Publish a candidate only into a new directory; existing data is never replaced."""
    if not root.is_absolute() or root.exists():
        raise ValueError("Parsed output requires a new absolute directory")
    if len(catalog.index.shards) != len(catalog.shards):
        raise ValueError("Parsed candidate index differs from shards")
    root.mkdir(parents=True)
    for pin, shard in zip(catalog.index.shards, catalog.shards, strict=True):
        path = member(root, pin.path)
        path.parent.mkdir(parents=True, exist_ok=True)
        cards, evidence = separate(shard)
        path.write_bytes(encode(cards))
        member(root, pin.evidence_path).write_bytes(encode(evidence))
    member(root, "index.json").write_bytes(encode(catalog.index))
    return load(root)
