"""Read complete immutable adoption histories before deriving any effective selection."""

import re
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, ValidationError

from sve_carddb.catalog.adoption_models import (
    AliasRecord,
    CatalogIndex,
    CatalogShard,
    DefaultRecord,
    DisplayIndex,
    DisplayShard,
    Record,
    RouteRecord,
    Shard,
    SymbolRecord,
    VocabularyRecord,
)
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import canonical, digest

if TYPE_CHECKING:
    from sve_carddb.registry.records import RecordData

Entry = Literal["catalog-adoptions", "display-overrides"]
_AREAS = {
    "vocabulary": ("vocabulary_adoption", "catalog-vocabulary-v1"),
    "languages": ("language_adoption", "catalog-language-v1"),
    "aliases": ("search_alias_adoption", "catalog-alias-v1"),
    "symbols": ("text_symbol_adoption", "catalog-symbol-v1"),
    "rules-names": ("rules_name_adoption", "catalog-rules-name-v1"),
    "routes": ("route_override_adoption", "display-route-v1"),
    "defaults": ("default_printing_adoption", "display-default-v1"),
}
_KEYS = {
    "vocabulary": {"kind", "code"},
    "language": {"code"},
    "text_symbol": {"id"},
    "keyword": {"id"},
    "stamp": {"id"},
    "product_family": {"id"},
    "card": {"id"},
    "face": {"id"},
    "printing": {"id"},
    "rules_name": {"id"},
}


def _json(model: RecordData) -> JsonValue:
    return model.model_dump(mode="json")


def ordered(values: tuple[RecordData, ...]) -> None:
    """Reject rather than normalize signed ordering or duplicate evidence."""
    keys = [canonical(_json(item)) for item in values]
    if keys != sorted(set(keys)):
        raise ValueError("Adoption array must be sorted and unique")


@dataclass(frozen=True)
class LoadedShard:
    path: str
    exact: bytes
    content_hash: str
    content: bytes

    def envelope(self) -> Shard:
        """Return detached typed data so callers cannot mutate an approved snapshot."""
        model = (
            CatalogShard if self.path.startswith("catalog-adoptions/") else DisplayShard
        )
        return model.model_validate_json(self.content)


@dataclass(frozen=True)
class AdoptionSnapshot:
    entry: Entry
    index_exact: bytes
    index_content: bytes
    shards: tuple[LoadedShard, ...]

    def records(self) -> tuple[tuple[Record, str], ...]:
        """Retain all historical members with their actual batch decisions."""
        return tuple(
            (record, shard.envelope().default_decision_id)
            for shard in self.shards
            for record in shard.envelope().records
        )

    def effective(self) -> tuple[tuple[Record, str], ...]:
        """Choose only the terminal member of each previously validated chain."""
        latest: dict[bytes, tuple[Record, str]] = {}
        for record, decision in self.records():
            key = subject_key(record)
            previous = latest.get(key)
            if (
                previous is None
                or previous[0].data.adoption_no < record.data.adoption_no
            ):
                latest[key] = record, decision
        return tuple(latest[key] for key in sorted(latest))

    def pins(self) -> dict[str, JsonValue]:
        """Keep exact YAML bytes distinct from canonical membership hashes."""
        return {
            "entry": self.entry,
            "index_hash": digest(self.index_exact),
            "index_canonical_hash": digest(self.index_content),
            "shards": [
                {
                    "path": shard.path,
                    "exact_hash": digest(shard.exact),
                    "canonical_hash": shard.content_hash,
                }
                for shard in self.shards
            ],
        }


def subject_key(record: Record) -> bytes:
    """Avoid delimiter collisions and exclude revision from a stable subject key."""
    return canonical([record.kind, _json(record.data.subject)])


def _safe(root: Path, path: Path) -> None:
    for part in (path, *path.parents):
        if part.is_symlink():
            raise ValueError("Symlink adoption input")
    if not path.is_relative_to(root) or not path.is_file():
        raise ValueError("Missing or unsafe adoption input")


def _model[T: RecordData](model: type[T], raw: JsonValue) -> T:
    try:
        return model.model_validate_json(canonical(raw))
    except ValidationError:
        raise ValueError("Invalid adoption fields") from None


def load_adoptions(root: Path, *, entry: Entry) -> AdoptionSnapshot:  # ruff: ignore[complex-structure,too-many-locals] -- complete entry closure is validated before projection
    """Validate the entire enabled entry, including every area and historical revision."""
    if entry not in {"catalog-adoptions", "display-overrides"}:
        raise ValueError("Unknown adoption entry")
    root = root.absolute()
    directory = root / entry
    path = directory / "index.yaml"
    _safe(root, path)
    raw = read_yaml(path)
    field = (
        "catalog_adoption_format"
        if entry == "catalog-adoptions"
        else "display_override_format"
    )
    _format(raw, field)
    index = (
        _model(CatalogIndex, raw)
        if entry == "catalog-adoptions"
        else _model(DisplayIndex, raw)
    )
    present: set[str] = set()
    for file in directory.rglob("*"):
        if file.is_symlink():
            raise ValueError("Symlink adoption input")
        if file.suffix.lower() in {".yaml", ".yml"} and file != path:
            present.add(file.relative_to(root).as_posix())
    areas = (
        "vocabulary|languages|aliases|symbols|rules-names"
        if entry == "catalog-adoptions"
        else "routes|defaults"
    )
    pattern = re.compile(rf"{entry}/({areas})/([A-Za-z0-9_-]+)/([0-9]{{3,}})\.yaml")
    sequences: dict[tuple[str, str], list[int]] = defaultdict(list)
    for name in index.includes:
        match = pattern.fullmatch(name)
        if match is None:
            raise ValueError("Unsafe or cross-entry adoption include")
        sequences[match[1], match[2]].append(int(match[3]))
    if present != set(index.includes):
        raise ValueError("Adoption indexed file closure differs from disk")
    for numbers in sequences.values():
        if sorted(numbers) != list(range(1, len(numbers) + 1)):
            raise ValueError("Adoption shard sequence must start at one without gaps")
    shards = []
    for name, checksum in sorted(index.includes.items()):
        file = root / name
        _safe(root, file)
        content = read_yaml(file)
        _format(content, field)
        if digest(canonical(content)) != checksum:
            raise ValueError("Adoption shard canonical hash mismatch")
        shard = (
            _model(CatalogShard, content)
            if entry == "catalog-adoptions"
            else _model(DisplayShard, content)
        )
        _check_shard(shard, name)
        shards.append(
            LoadedShard(name, file.read_bytes(), checksum, canonical(content))
        )
    snapshot = AdoptionSnapshot(entry, path.read_bytes(), canonical(raw), tuple(shards))
    _chains(snapshot)
    return snapshot


def _format(raw: JsonValue, field: str) -> None:
    if not isinstance(raw, dict) or type(raw.get(field)) is not int or raw[field] != 1:
        raise ValueError("Adoption format must be integer one")


def _check_shard(shard: Shard, path: str) -> None:  # ruff: ignore[complex-structure,too-many-branches] -- independent envelope guards expose precise boundary failures
    area, filing = Path(path).parts[1:3]
    kind, policy = _AREAS[area]
    decision = shard.decisions[0]
    if decision.category != kind or decision.policy_id != policy:
        raise ValueError("Adoption category/policy does not match area")
    keys = [record.record_key for record in shard.records]
    if keys != sorted(set(keys)):
        raise ValueError("Adoption record keys must be sorted and unique")
    ordered(shard.review_context.source_batches)
    for record in shard.records:
        if record.kind != kind or record.filing_key != filing:
            raise ValueError("Adoption kind/filing key does not match path")
        data = record.data
        expected = canonical(
            [record.kind, _json(data.subject), data.adoption_no]
        ).decode()
        if record.record_key != expected:
            raise ValueError("Adoption record key does not match subject/revision")
        if data.review_context_hash != digest(canonical(_json(shard.review_context))):
            raise ValueError("Adoption review context hash mismatch")
        ordered(record.evidence)
        ordered(data.dependencies)
        for dependency in data.dependencies:
            if set(dependency.key) != _KEYS[dependency.table]:
                raise ValueError("Adoption dependency primary key fields mismatch")
            if any(not isinstance(v, str) or not v for v in dependency.key.values()):
                raise ValueError("Adoption dependency keys must be nonempty text")
        batches = {
            (b.store_id, b.batch_id) for b in shard.review_context.source_batches
        }
        for evidence in record.evidence:
            ref = (
                evidence.source_ref
                if hasattr(evidence, "source_ref")
                else evidence.image_ref
            )
            if (ref.store_id, ref.batch_id) not in batches:
                raise ValueError("Adoption evidence batch absent from review context")
    members = tuple((r.record_key, digest(canonical(_json(r)))) for r in shard.records)
    checksum = digest(canonical([[key, value] for key, value in members]))
    if decision.members != members:
        raise ValueError("Adoption exact decision members mismatch")
    if decision.membership_hash != checksum:
        raise ValueError("Adoption membership hash mismatch")
    if decision.id != "d:" + checksum.removeprefix("sha256:"):
        raise ValueError("Adoption decision ID mismatch")
    if shard.default_decision_id != decision.id:
        raise ValueError("Adoption default decision ID mismatch")
    if decision.sample_ids != tuple(key for key, _ in members):
        raise ValueError("Adoption checked members must cover the exact batch")


def _chains(snapshot: AdoptionSnapshot) -> None:  # ruff: ignore[complex-structure,too-many-branches,too-many-statements] -- history and terminal uniqueness guards share one complete chain inventory
    groups: dict[bytes, list[tuple[Record, str]]] = defaultdict(list)
    seen: set[str] = set()
    decisions: set[str] = set()
    for shard in snapshot.shards:
        decision = shard.envelope().default_decision_id
        if decision in decisions:
            raise ValueError("Duplicate adoption decision")
        decisions.add(decision)
    for record, decision in snapshot.records():
        if record.record_key in seen:
            raise ValueError("Duplicate adoption record")
        seen.add(record.record_key)
        groups[subject_key(record)].append((record, decision))
    symbol_codes: dict[tuple[str, str], str] = {}
    mappings: dict[tuple[str, str, str, str], str] = {}
    normalizers: set[bytes] = set()
    for history in groups.values():
        history.sort(key=lambda pair: pair[0].data.adoption_no)
        previous: tuple[Record, str] | None = None
        route_target: str | None = None
        for number, (record, decision) in enumerate(history, 1):
            data = record.data
            if data.adoption_no != number:
                raise ValueError("Adoption revision sequence gap or fork")
            if previous is None:
                if data.predecessor is not None or data.value is None:
                    raise ValueError(
                        "Initial adoption requires value and null predecessor"
                    )
            elif data.predecessor is None or (
                data.predecessor.record_key,
                data.predecessor.record_hash,
                data.predecessor.decision_id,
            ) != (
                previous[0].record_key,
                digest(canonical(_json(previous[0]))),
                previous[1],
            ):
                raise ValueError("Adoption exact predecessor mismatch")
            if isinstance(record, SymbolRecord) and record.data.value is not None:
                code = record.data.value.code
                if (
                    symbol_codes.setdefault(("id", record.data.subject.id), code)
                    != code
                ):
                    raise ValueError("Symbol stable code cannot be reassigned")
                owner = symbol_codes.setdefault(("code", code), record.data.subject.id)
                if owner != record.data.subject.id:
                    raise ValueError("Symbol code/id must be one-to-one across history")
            if isinstance(record, RouteRecord):
                route_target = _permanent_route(record, route_target)
            previous = record, decision
        assert previous is not None
        record = previous[0]
        if isinstance(record, VocabularyRecord) and record.data.value is not None:
            value = record.data.value
            ordered(value.raw_mappings)
            if value.active:
                for raw in value.raw_mappings:
                    key = record.data.subject.kind, raw.region, raw.lang, raw.raw
                    code = mappings.setdefault(key, record.data.subject.code)
                    if code != record.data.subject.code:
                        raise ValueError(
                            "Exact vocabulary raw maps to multiple active codes"
                        )
        if isinstance(record, AliasRecord) and record.data.value is not None:
            normalizers.add(canonical(_json(record.data.value.normalizer)))
        if isinstance(record, DefaultRecord) and record.data.value is not None:
            candidates = record.data.value.candidates
            if candidates != tuple(sorted(set(candidates))):
                raise ValueError("Default candidates must be sorted and unique")
    if len(normalizers) > 1:
        raise ValueError("Effective aliases cannot mix normalizer pins")


def _permanent_route(record: RouteRecord, target: str | None) -> str:
    """No standalone loader can authorize a permanent entry change without its comparison."""
    value = record.data.value
    if value is None or (target is not None and value.printing_id != target):
        raise ValueError(
            "Route target change or withdrawal requires permanent-entry comparison"
        )
    return value.printing_id
