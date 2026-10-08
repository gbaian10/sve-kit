"""Read complete immutable adoption histories before deriving any effective selection."""

from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, ValidationError

from sve_carddb.authored_files import shards
from sve_carddb.catalog.adoption_models import (
    AliasRecord,
    CatalogShard,
    DefaultRecord,
    DisplayShard,
    Record,
    RouteRecord,
    Shard,
    SymbolRecord,
    VocabularyRecord,
)
from sve_carddb.catalog.current_models import Shard as CurrentShard
from sve_carddb.catalog.current_models import key as current_key
from sve_carddb.snapshot.values import canonical, digest, object_value, parse

if TYPE_CHECKING:
    from sve_carddb.catalog.current_models import Record as CurrentRecord
    from sve_carddb.registry.records import RecordData

CURRENT_FORMAT = 2
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
    if len(keys) != len(set(keys)):
        raise ValueError("Adoption array must be unique")


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
    shards: tuple[LoadedShard, ...]

    def current_records(self) -> tuple[CurrentRecord, ...]:
        """Read current vocabulary values without creating adoption envelopes."""
        return tuple(
            sorted(
                (
                    r
                    for s in self.shards
                    if object_value(parse(s.content)).get("catalog_adoption_format")
                    == CURRENT_FORMAT
                    for r in CurrentShard.model_validate_json(s.content).records
                ),
                key=lambda r: r.record_key,
            )
        )

    def records(self) -> tuple[tuple[Record, str], ...]:
        """Retain all historical members with their actual batch decisions."""
        return tuple(
            (record, shard.envelope().default_decision_id)
            for shard in self.shards
            if object_value(parse(shard.content)).get("catalog_adoption_format")
            != CURRENT_FORMAT
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


def subject_key(record: Record) -> bytes:
    """Avoid delimiter collisions and exclude revision from a stable subject key."""
    return canonical([record.kind, _json(record.data.subject)])


def _model[T: RecordData](model: type[T], raw: JsonValue) -> T:
    try:
        return model.model_validate_json(canonical(raw))
    except ValidationError:
        raise ValueError("Invalid adoption fields") from None


def load_adoptions(root: Path, *, entry: Entry) -> AdoptionSnapshot:
    """Validate the entire enabled entry, including every area and historical revision."""
    if entry not in {"catalog-adoptions", "display-overrides"}:
        raise ValueError("Unknown adoption entry")
    field = (
        "catalog_adoption_format"
        if entry == "catalog-adoptions"
        else "display_override_format"
    )
    areas = (
        ("vocabulary", "languages", "aliases", "symbols", "rules-names")
        if entry == "catalog-adoptions"
        else ("routes", "defaults")
    )
    loaded = []
    for name, exact, encoded in shards(
        root, tuple(entry + "/" + area for area in areas)
    ):
        content = parse(encoded)
        _format(content, field)
        if (
            entry == "catalog-adoptions"
            and object_value(content)[field] == CURRENT_FORMAT
        ):
            current = _model(CurrentShard, content)
            keys = [r.record_key for r in current.records]
            if len(keys) != len(set(keys)):
                raise ValueError("Current catalog records must be unique")
            for record in current.records:
                area = (
                    "vocabulary"
                    if record.kind == "vocabulary_adoption"
                    else "languages"
                )
                if Path(name).parts[1] != area or record.record_key != current_key(
                    record
                ):
                    raise ValueError("Current catalog key or area mismatch")
        else:
            shard = _model(
                CatalogShard if entry == "catalog-adoptions" else DisplayShard, content
            )
            _check_shard(shard, name)
        loaded.append(LoadedShard(name, exact, digest(encoded), encoded))
    snapshot = AdoptionSnapshot(entry, tuple(loaded))
    _chains(snapshot)
    keys = [r.record_key for r in snapshot.current_records()]
    legacy_keys = [subject_key(r).decode() for r, _ in snapshot.effective()]
    if len(set(keys + legacy_keys)) != len(keys + legacy_keys):
        raise ValueError("Duplicate current catalog selection key")
    return snapshot


def _format(raw: JsonValue, field: str) -> None:
    if (
        not isinstance(raw, dict)
        or type(raw.get(field)) is not int
        or raw[field]
        not in ({1, CURRENT_FORMAT} if field == "catalog_adoption_format" else {1})
    ):
        raise ValueError("Adoption format must be integer one")


def _check_shard(shard: Shard, path: str) -> None:  # ruff: ignore[complex-structure,too-many-branches] -- independent envelope guards expose precise boundary failures
    area, filing = Path(path).parts[1:3]
    kind, policy = _AREAS[area]
    decision = shard.decisions[0]
    if decision.category != kind or decision.policy_id != policy:
        raise ValueError("Adoption category/policy does not match area")
    keys = [record.record_key for record in shard.records]
    if len(keys) != len(set(keys)):
        raise ValueError("Adoption record keys must be unique")
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
        batches = {b.batch_id for b in shard.review_context.source_batches}
        for evidence in record.evidence:
            ref = (
                evidence.source_ref
                if hasattr(evidence, "source_ref")
                else evidence.image_ref
            )
            if ref.batch_id not in batches:
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


def _chains(snapshot: AdoptionSnapshot) -> None:  # ruff: ignore[complex-structure,too-many-branches,too-many-statements,too-many-locals] -- history and terminal uniqueness guards share one complete chain inventory
    groups: dict[bytes, list[tuple[Record, str]]] = defaultdict(list)
    seen: set[str] = set()
    decisions: set[str] = set()
    for shard in snapshot.shards:
        if (
            object_value(parse(shard.content)).get("catalog_adoption_format")
            == CURRENT_FORMAT
        ):
            continue
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
    mappings: dict[tuple[str, str, str, str], tuple[str, tuple[str, ...]]] = {}
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
                    selection = record.data.subject.code, raw.special_kinds
                    previous_mapping = mappings.get(key)
                    if previous_mapping is not None and previous_mapping != selection:
                        raise ValueError(
                            "Exact vocabulary raw maps to multiple active code/marker pairs"
                        )
                    if previous_mapping == selection:
                        raise ValueError("Duplicate active vocabulary raw mapping")
                    mappings[key] = selection
        if isinstance(record, AliasRecord) and record.data.value is not None:
            normalizers.add(canonical(_json(record.data.value.normalizer)))
        if isinstance(record, DefaultRecord) and record.data.value is not None:
            candidates = record.data.value.candidates
            if candidates != tuple(sorted(set(candidates))):
                raise ValueError("Default candidates must be unique")
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
