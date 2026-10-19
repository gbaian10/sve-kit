"""Validate the whole reviewed input closure before exposing any source document."""

from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

from sve_carddb.core.json import digest
from sve_carddb.domains.parsed_cards import batches as batch_files
from sve_carddb.domains.parsed_cards.codec import decode, encode
from sve_carddb.domains.parsed_cards.filing import file_path, number_range
from sve_carddb.domains.parsed_cards.models import (
    MAX_BYTES,
    BatchFile,
    BatchPin,
    BatchSelection,
    Card,
    FilePair,
    FilePin,
    Index,
    ParsedObservation,
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
    files: tuple[FilePair, ...]
    batches: tuple[BatchSelection, ...]

    def observations(self) -> dict[str, ParsedObservation]:
        """Expose only validated complete source observations."""
        result = {}
        for pair in self.files:
            for card in pair.cards:
                for value in card.observations:
                    version = value.evidence.descriptor.id
                    if version in result:
                        raise ValueError("Duplicate parsed source version")
                    result[version] = value
        return result

    def filing(self) -> dict[tuple[Region, str], str]:
        """Carry reviewed family assignments into incremental requests."""
        return {(p.region, c.card_no): p.family for p in self.files for c in p.cards}

    def validate_batches(self) -> None:
        """Check exact selection closure without reconstructing a private inventory."""
        observations = self.observations()
        selected = set()
        if tuple(b.id for b in self.batches) != tuple(b.id for b in self.index.batches):
            raise ValueError("Parsed batch index differs from selection files")
        for batch in self.batches:
            current_urls = set()
            for version, current in batch.versions.items():
                value = observations.get(version)
                if value is None or value.document.region != batch.region:
                    raise ValueError(
                        "Parsed batch has an absent or foreign source version"
                    )
                selected.add(version)
                if current:
                    url = value.evidence.descriptor.url
                    if url in current_urls:
                        raise ValueError("Multiple current versions of a parsed page")
                    current_urls.add(url)
        if selected != observations.keys():
            raise ValueError("Parsed observation is absent from every batch")

    def validate_update(self, previous: Catalog) -> None:
        """A self-consistent new index cannot authorize deleting old source evidence."""
        current = self.observations()
        for version, old in previous.observations().items():
            new = current.get(version)
            if new is None or new.content_hash != old.content_hash:
                raise ValueError(
                    "Parsed update deletes or replaces a historical observation"
                )
        old_batches = {b.id: b for b in previous.batches}
        if any(
            old_batches.get(b.id, b) != b for b in self.batches
        ) or not old_batches.keys() <= {b.id for b in self.batches}:
            raise ValueError("Parsed update changes a historical batch")
        old_filing = {
            (s.region, c.card_no): (s.family, s.range)
            for s in previous.files
            for c in s.cards
        }
        if any(
            old_filing.get((s.region, c.card_no), (s.family, s.range))
            != (s.family, s.range)
            for s in self.files
            for c in s.cards
        ):
            raise ValueError("Parsed update changes existing filing")


@dataclass
class _Closure:
    cards: set[tuple[Region, str]] = field(default_factory=set)
    versions: set[str] = field(default_factory=set)

    def observation(self, value: ParsedObservation) -> None:
        descriptor = value.evidence.descriptor
        if descriptor.id in self.versions:
            raise ValueError("Duplicate parsed source version")
        self.versions.add(descriptor.id)

    def card(self, card: Card, pair: FilePair, path: str) -> None:
        key = pair.region, card.card_no
        expected = file_path(pair.region, pair.family, card.card_no)
        interval = number_range(card.card_no, pair.region)
        if key in self.cards or path != expected or pair.range != interval:
            raise ValueError("Duplicate or incorrectly filed parsed card")
        self.cards.add(key)
        for observation in card.observations:
            self.observation(observation)


def _read_pair(root: Path, pin: FilePin) -> FilePair:
    raw = member(root, pin.path).read_bytes()
    if len(raw) != pin.bytes or digest(raw) != pin.sha256:
        raise ValueError("Parsed pair size/hash mismatch")
    evidence_raw = member(root, pin.evidence_path).read_bytes()
    if (
        len(evidence_raw) != pin.evidence_bytes
        or digest(evidence_raw) != pin.evidence_sha256
    ):
        raise ValueError("Parsed evidence size/hash mismatch")
    pair = combine(decode(CardFile, raw), decode(EvidenceFile, evidence_raw))
    cards, evidence = separate(pair)
    if encode(cards) != raw or encode(evidence) != evidence_raw:
        raise ValueError("Parsed file pair has noncanonical map order")
    if (len(pair.cards), sum(len(c.observations) for c in pair.cards)) != (
        pin.cards,
        pin.observations,
    ):
        raise ValueError("Parsed pair member count mismatch")
    return pair


def load(root: Path) -> Catalog:
    """Read no raw source, manifest SQLite, network resource, or implicit fallback."""
    if not root.is_absolute() or root.is_symlink():
        raise ValueError("Parsed data root must be absolute and not a symlink")
    index_raw = member(root, "index.json").read_bytes()
    if len(index_raw) > MAX_BYTES:
        raise ValueError("Parsed index exceeds 1 MiB")
    index = decode(Index, index_raw)
    closure = _Closure()
    files: list[FilePair] = []
    for pin in index.files:
        pair = _read_pair(root, pin)
        for card in pair.cards:
            closure.card(card, pair, pin.path)
        files.append(pair)
    batches = tuple(_read_batch(root, pin) for pin in index.batches)
    expected_paths = (
        {p.path for p in index.files}
        | {p.evidence_path for p in index.files}
        | {p.path(i) for p in index.batches for i in range(len(p.parts))}
        | {"index.json"}
    )
    actual_paths = {p.relative_to(root).as_posix() for p in root.rglob("*.json")}
    if (root / "errata").exists():
        expected_paths |= {"errata/index.json", "errata/jp.json", "errata/en.json"}
    if expected_paths != actual_paths:
        raise ValueError("Parsed directory contains missing or unlisted JSON members")
    result = Catalog(index, tuple(files), batches)
    result.validate_batches()
    return result


def write(root: Path, catalog: Catalog) -> Catalog:
    """Publish a candidate only into a new directory; existing data is never replaced."""
    if not root.is_absolute() or root.exists():
        raise ValueError("Parsed output requires a new absolute directory")
    if len(catalog.index.files) != len(catalog.files):
        raise ValueError("Parsed candidate index differs from files")
    root.mkdir(parents=True)
    for pin, pair in zip(catalog.index.files, catalog.files, strict=True):
        path = member(root, pin.path)
        path.parent.mkdir(parents=True, exist_ok=True)
        cards, evidence = separate(pair)
        path.write_bytes(encode(cards))
        member(root, pin.evidence_path).write_bytes(encode(evidence))
    for batch_pin, batch in zip(catalog.index.batches, catalog.batches, strict=True):
        for part in batch_files.parts(batch):
            path = member(root, batch_pin.path(part.part))
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(encode(part))
    member(root, "index.json").write_bytes(encode(catalog.index))
    return load(root)


def _read_batch(root: Path, pin: BatchPin) -> BatchSelection:
    members = []
    for ordinal, part_pin in enumerate(pin.parts):
        raw = member(root, pin.path(ordinal)).read_bytes()
        if len(raw) != part_pin.bytes or digest(raw) != part_pin.sha256:
            raise ValueError("Parsed batch size/hash mismatch")
        batch = decode(BatchFile, raw)
        if batch.id != pin.id:
            raise ValueError("Parsed batch identity differs from pin")
        members.append(batch)
    return batch_files.join(tuple(members))
