"""Strict YAML boundaries and append-only, checksummed registry shards."""

import hashlib
import io
import os
import tempfile
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue, model_validator
from ruamel.yaml import YAML

from sve_carddb.registry.allocation import (
    ALLOCATION_POLICY,
    REGION_RANGES,
    cursors,
    region_allocations,
)
from sve_carddb.registry.inputs import JSON_VALUE, canonical, digest
from sve_carddb.registry.transitions.files import require_empty_transitions
from sve_carddb.registry.yaml_reader import parse_yaml

if TYPE_CHECKING:
    from collections.abc import Callable

MAX_BYTES = 1_048_576
TARGET_BYTES = 524_288


class Entry(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    record_key: str
    kind: Literal[
        "card",
        "face",
        "printing",
        "card_int_id",
        "region_mapping_review",
        "art",
        "card_related",
        "source_correction",
    ]
    owner: str
    data: dict[str, JsonValue]


class Decision(BaseModel):
    model_config = ConfigDict(extra="forbid")
    id: str
    state: Literal["confirmed", "proposed"]
    scope: Literal["batch"] = "batch"
    category: Literal["identity_registry"] = "identity_registry"
    policy_id: str
    membership_hash: str
    members: list[tuple[str, str]]
    sample_ids: list[str]


class Shard(BaseModel):
    model_config = ConfigDict(extra="forbid")
    authored_format: Literal[1] = 1
    kind: Literal["registry_shard"] = "registry_shard"
    default_decision_id: str | None
    records: list[Entry]
    decisions: list[Decision]


class Index(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    authored_format: Literal[2] = 2
    kind: Literal["registry_index"] = "registry_index"
    allocation_policy: str = ALLOCATION_POLICY
    next_int_id: dict[str, int] = Field(
        default_factory=lambda: {
            region: bounds.start for region, bounds in sorted(REGION_RANGES.items())
        }
    )
    includes: dict[str, str] = Field(default_factory=dict)

    @model_validator(mode="after")
    def _known_ranges(self) -> Index:
        if self.allocation_policy != ALLOCATION_POLICY:
            raise ValueError(f"Unknown allocation policy: {self.allocation_policy}")
        if set(self.next_int_id) != set(REGION_RANGES) or any(
            not bounds.start <= self.next_int_id[region] <= bounds.end + 1
            for region, bounds in REGION_RANGES.items()
        ):
            raise ValueError("Index cursors disagree with the allocation policy")
        return self


def yaml_parser() -> YAML:
    """Limit the safe pure parser to YAML 1.2 core scalar types."""
    yaml = YAML(typ="safe", pure=True)
    yaml.version = (1, 2)
    yaml.allow_duplicate_keys = False
    resolver = yaml.resolver.versioned_resolver
    allowed = {"tag:yaml.org,2002:" + name for name in ("bool", "int", "float", "null")}
    for key, rules in list(resolver.items()):
        resolver[key] = [(tag, pattern) for tag, pattern in rules if tag in allowed]
    return yaml


def read_yaml(path: Path) -> JsonValue:
    """Require bounded UTF-8 YAML, JSON values and finite canonical content."""
    return _read_yaml_content(path)[0]


def _read_yaml_content(path: Path) -> tuple[JsonValue, bytes]:
    data = path.read_bytes()
    if len(data) >= MAX_BYTES:
        raise ValueError(f"Oversized YAML: {path.name}")
    value = parse_yaml(data)
    result = JSON_VALUE.validate_python(value, strict=True)
    return result, canonical(result)


type OmittedFields = dict[str, bool | OmittedFields] | dict[int, bool | OmittedFields]


def _empty_defaults(value: object) -> OmittedFields:
    if isinstance(value, BaseModel):
        omitted: dict[str, bool | OmittedFields] = {}
        for name, info in type(value).model_fields.items():
            item = getattr(value, name)
            if (
                not info.is_required()
                and info.default in {None, ""}
                and type(item) is type(info.default)
                and item == info.default
            ):
                omitted[name] = True
            elif nested := _empty_defaults(item):
                omitted[name] = nested
        return omitted
    if isinstance(value, (list, tuple)):
        indexed: dict[int, bool | OmittedFields] = {
            index: nested
            for index, item in enumerate(value)
            if (nested := _empty_defaults(item))
        }
        return indexed
    return {}


def encode(model: BaseModel) -> bytes:
    """Write stable block YAML with quoted dates and no object aliases."""
    yaml = YAML()
    yaml.default_flow_style = False
    yaml.width = 1000
    yaml.indent(mapping=2, sequence=4, offset=2)
    stream = io.StringIO()
    # Only empty optional values are omitted; other defaults can be required wire fields.
    yaml.dump(model.model_dump(mode="json", exclude=_empty_defaults(model)), stream)
    return stream.getvalue().encode()


def members(records: list[Entry]) -> list[tuple[str, str]]:
    """Hash semantic records, excluding inherited decision pointers."""
    return sorted(
        (record.record_key, digest(record.model_dump(mode="json")))
        for record in records
    )


def member_hash(items: list[tuple[str, str]]) -> str:
    """Hash sorted (record key, semantic hash) pairs."""
    return digest([[key, value] for key, value in items])


@dataclass(frozen=True)
class LoadedShard:
    path: str
    content_hash: str
    content: bytes
    _ordered_content: bytes
    exact_content: bytes

    def envelope(self) -> Shard:
        """Return a detached copy; canonical input remains immutable."""
        return Shard.model_validate_json(self._ordered_content)


@dataclass(frozen=True)
class RegistryFiles:
    index_content: bytes
    shards: tuple[LoadedShard, ...]

    def index(self) -> Index:
        """Return a detached index, including the original global cursors."""
        return Index.model_validate_json(self.index_content)


def read_registry_files(root: Path) -> RegistryFiles:
    """Read checked envelopes once, without discarding their source or membership."""
    require_empty_transitions(root)
    return read_base_files(root)


def read_base_files(root: Path) -> RegistryFiles:
    """Read original envelopes for replay only; this is not an effective registry.

    Consumers must use load_registry or the transition replay boundary. Reading
    these files alone does not validate or apply the independent transition log.
    """
    path = root / "ids" / "index.yaml"
    if not path.exists():
        if any((root / "registry").glob("**/*.yaml")) or any(
            (root / "ids").glob("**/*.yaml")
        ):
            raise ValueError("Unindexed registry files; recover before allocating IDs")
        return RegistryFiles(canonical(Index().model_dump(mode="json")), ())
    _safe_file(root, path)
    raw_index, index_content = _read_yaml_content(path)
    _wire_fields(
        raw_index,
        2,
        {"authored_format", "kind", "allocation_policy", "next_int_id", "includes"},
    )
    index = Index.model_validate(raw_index)
    present = {
        file.relative_to(root).as_posix()
        for directory in (root / "registry", root / "ids")
        for file in directory.rglob("*.yaml")
        if file != path
    }
    if present != set(index.includes):
        raise ValueError(
            "Indexed file closure differs from disk; recover interrupted writes"
        )
    shards = []
    keys: set[str] = set()
    for name, checksum in sorted(index.includes.items()):
        relative = Path(name)
        if (
            relative.is_absolute()
            or not relative.parts
            or ".." in relative.parts
            or relative.parts[0] not in {"registry", "ids"}
        ):
            raise ValueError(f"Unsafe include: {name}")
        file = root / relative
        _safe_file(root, file)
        raw, content = _read_yaml_content(file)
        if "sha256:" + hashlib.sha256(content).hexdigest() != checksum:
            raise ValueError(f"Modified immutable shard: {name}")
        _wire_fields(
            raw,
            1,
            {"authored_format", "kind", "default_decision_id", "records", "decisions"},
        )
        shard = Shard.model_validate(raw)
        _check_decision(shard)
        for entry in shard.records:
            if entry.record_key in keys:
                raise ValueError(f"Duplicate record: {entry.record_key}")
            keys.add(entry.record_key)
        shards.append(
            LoadedShard(
                name,
                checksum,
                content,
                shard.model_dump_json().encode(),
                file.read_bytes(),
            )
        )
    return RegistryFiles(index_content, tuple(shards))


def _wire_fields(raw: JsonValue, version: int, fields: set[str]) -> None:
    if (
        not isinstance(raw, dict)
        or set(raw) != fields
        or type(raw.get("authored_format")) is not int
        or raw["authored_format"] != version
    ):
        raise ValueError("Invalid registry envelope fields or format")


def _safe_file(root: Path, path: Path) -> None:
    if path.is_symlink() or not path.resolve().is_relative_to(root.resolve()):
        raise ValueError("Unsafe registry file")


def load(root: Path) -> tuple[Index, dict[str, Entry]]:
    """Keep the append tool's legacy API; builders should use snapshot.load_registry."""
    files = read_registry_files(root)
    return files.index(), {
        entry.record_key: entry
        for shard in files.shards
        for entry in shard.envelope().records
    }


def _check_decision(shard: Shard) -> None:
    if all(record.kind == "card_int_id" for record in shard.records):
        if shard.default_decision_id is not None or shard.decisions:
            raise ValueError("Deterministic allocations must not carry a decision")
        return
    if any(record.kind == "card_int_id" for record in shard.records):
        raise ValueError("Allocations cannot share a decision shard")
    if len(shard.decisions) != 1:
        raise ValueError("A shard needs exactly one batch decision")
    decision = shard.decisions[0]
    expected = members(shard.records)
    if (
        decision.id != "d:" + decision.membership_hash.removeprefix("sha256:")
        or decision.id != shard.default_decision_id
        or decision.members != expected
        or decision.membership_hash != member_hash(expected)
    ):
        raise ValueError("Decision membership mismatch")
    for record in shard.records:
        if record.kind == "source_correction":
            required = (
                "proposed" if record.data["state"] == "needs_review" else "confirmed"
            )
            if decision.state != required:
                raise ValueError("Correction state disagrees with decision")
    _check_samples(decision, [key for key, _ in expected])


def _check_samples(decision: Decision, checked: list[str]) -> None:
    if len(set(decision.sample_ids)) != len(decision.sample_ids) or not set(
        decision.sample_ids
    ) <= set(checked):
        raise ValueError("Checked members must be a unique subset of the batch")
    if decision.state == "confirmed" and (decision.sample_ids != checked):
        raise ValueError("Confirmed batch must explicitly check every member")


def _shard(records: list[Entry]) -> Shard:
    if records[0].kind == "card_int_id":
        return Shard(default_decision_id=None, records=records, decisions=[])
    items = members(records)
    checksum = member_hash(items)
    proposed = (
        records[0].kind == "source_correction"
        and records[0].data["state"] == "needs_review"
    )
    decision = Decision(
        id="d:" + checksum.removeprefix("sha256:"),
        state="proposed" if proposed else "confirmed",
        policy_id="identity-init-2026-09-28-v1",
        membership_hash=checksum,
        members=items,
        sample_ids=[] if proposed else [key for key, _ in items],
    )
    return Shard(default_decision_id=decision.id, records=records, decisions=[decision])


def record_order(entries: list[Entry]) -> Callable[[Entry], tuple[str, ...]]:
    """Order shard records by their printing's card number, else by record key.

    Only a shard's own records are ordered; appended shards never re-sort old files.
    """
    printings: dict[str, tuple[str, ...]] = {}
    for entry in entries:
        if entry.kind == "printing":
            data = entry.data
            printings[_text(data, "id")] = tuple(
                _text(data, name) for name in ("region", "card_no", "variant_key", "id")
            )
    cards: dict[str, tuple[str, ...]] = {}
    for entry in entries:
        if entry.kind == "printing":
            card = _text(entry.data, "card_id")
            anchor = printings[_text(entry.data, "id")]
            cards[card] = min(cards.get(card, anchor), anchor)

    def key(entry: Entry) -> tuple[str, ...]:
        data = entry.data
        if entry.kind == "art":
            uses = data["uses"]
            first = uses[0] if isinstance(uses, list) and uses else None
            data = first if isinstance(first, dict) else {}
        table, field = _ANCHORS[entry.kind]
        base = (printings if table == "printing" else cards).get(_text(data, field), ())
        if entry.kind == "face":
            base = (*base, str(entry.data["ordinal"]))
        return (*base, entry.record_key)

    return key


_ANCHORS = {
    "printing": ("printing", "id"),
    "card_int_id": ("printing", "printing_id"),
    "source_correction": ("printing", "printing_id"),
    "art": ("printing", "printing_id"),
    "card": ("card", "id"),
    "face": ("card", "card_id"),
    "region_mapping_review": ("card", "card_id"),
    "card_related": ("card", "from_card_id"),
}


def _text(data: dict[str, JsonValue], key: str) -> str:
    value = data.get(key)
    return value if isinstance(value, str) else ""


def _area(entry: Entry) -> str:
    area = "ids" if entry.kind == "card_int_id" else "registry/" + entry.kind
    if entry.kind == "source_correction":
        area += "/" + str(entry.data["state"])
    return area


def plan_files(
    root: Path,
    entries: list[Entry],
    *,
    loaded: tuple[Index, dict[str, Entry]] | None = None,
) -> dict[Path, bytes]:
    """Preserve all old entries and shards; only append newly allocated state."""
    require_empty_transitions(root)
    index, old = load(root) if loaded is None else loaded
    new: dict[tuple[str, str], list[Entry]] = defaultdict(list)
    for entry in entries:
        if entry.record_key in old:
            if entry != old[entry.record_key]:
                raise ValueError(
                    f"Existing registry record changed; explicit repair required: {entry.record_key}"
                )
        else:
            new[_area(entry), entry.owner].append(entry)
    if set(old) - {entry.record_key for entry in entries}:
        raise ValueError("Input would delete existing registry records")
    order = record_order(entries)
    files: dict[Path, bytes] = {}
    for (area, owner), records in sorted(new.items()):
        directory = _directory(root, area, owner)
        number = (
            max((int(path.stem) for path in directory.glob("[0-9]*.yaml")), default=0)
            + 1
        )
        _split(directory, sorted(records, key=order), number, files)
    if not files:
        return {}
    return _finish(root, index, entries, files)


def relayout(root: Path, entries: list[Entry]) -> dict[Path, bytes]:
    """Lay out a complete registry from scratch, grouped by (area, owner).

    Only for the one-time pre-publication reshard; normal runs append via plan_files.
    """
    require_empty_transitions(root)
    groups: dict[tuple[str, str], list[Entry]] = defaultdict(list)
    for entry in entries:
        groups[_area(entry), entry.owner].append(entry)
    order = record_order(entries)
    files: dict[Path, bytes] = {}
    for group, records in sorted(groups.items()):
        directory = _directory(root, *group)
        _split(directory, sorted(records, key=order), 1, files)
    return _finish(root, Index(), entries, files)


def _finish(
    root: Path, index: Index, entries: list[Entry], files: dict[Path, bytes]
) -> dict[Path, bytes]:
    for path, data in files.items():
        parsed: object = yaml_parser().load(data)
        index.includes[path.relative_to(root).as_posix()] = digest(
            JSON_VALUE.validate_python(parsed)
        )
    index.includes = dict(sorted(index.includes.items()))
    after = cursors(region_allocations(entries))
    if any(after[region] < index.next_int_id[region] for region in after):
        raise ValueError("Allocation high-water mark would move backwards")
    index = Index.model_validate(index.model_dump() | {"next_int_id": after})
    files[root / "ids" / "index.yaml"] = encode(index)
    if any(len(data) >= MAX_BYTES for data in files.values()):
        raise ValueError("Registry file exceeds 1 MiB")
    return files


def _directory(root: Path, area: str, owner: str) -> Path:
    if not owner or any(
        char not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-"
        for char in owner
    ):
        raise ValueError(f"Unsafe owner: {owner}")
    return root / area / owner


def _split(
    directory: Path,
    records: list[Entry],
    number: int,
    files: dict[Path, bytes],
) -> None:
    """Fill each shard up to the target, measured on its final encoded YAML."""
    while records:
        count, data = _fit(records)
        files[directory / f"{number:03}.yaml"] = data
        records, number = records[count:], number + 1


def _fit(records: list[Entry]) -> tuple[int, bytes]:
    cache: dict[int, bytes] = {}

    def size(count: int) -> int:
        if count not in cache:
            cache[count] = encode(_shard(records[:count]))
        return len(cache[count])

    if size(len(records)) <= TARGET_BYTES:
        return len(records), cache[len(records)]
    if size(1) >= MAX_BYTES:
        raise ValueError(
            f"Single registry record exceeds 1 MiB: {records[0].record_key}"
        )
    good, bad, interpolate = 1, len(records), True
    while bad - good > 1:
        # Size grows almost linearly; alternate with bisection to bound the steps.
        guess = (
            good
            + (bad - good) * (TARGET_BYTES - size(good)) // (size(bad) - size(good))
            if interpolate
            else (good + bad) // 2
        )
        middle, interpolate = min(max(guess, good + 1), bad - 1), not interpolate
        if size(middle) <= TARGET_BYTES:
            good = middle
        else:
            bad = middle
    return good, cache[good]


def write_files(files: dict[Path, bytes]) -> None:
    """Install shards first and the index last; an interrupted run fails closed."""
    for path, data in files.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        fd, name = tempfile.mkstemp(dir=path.parent, prefix=".registry-")
        temporary = Path(name)
        try:
            with os.fdopen(fd, "wb") as stream:
                stream.write(data)
                stream.flush()
                os.fsync(stream.fileno())
            temporary.chmod(0o644)
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)
