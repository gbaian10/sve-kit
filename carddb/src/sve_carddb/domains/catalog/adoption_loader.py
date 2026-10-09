"""Read current catalog/display values from the enabled authored data areas."""

import re
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Literal

from pydantic import ValidationError

from sve_carddb.core.authored import check_path, read, require_directory
from sve_carddb.core.json import canonical, digest, object_value, parse
from sve_carddb.domains.catalog.records import (
    AliasRecord,
    DefaultRecord,
    DisplayShard,
    Record,
    Shard,
    SymbolRecord,
)

if TYPE_CHECKING:
    from sve_carddb.core.models import RecordData

CURRENT_FORMAT = 2
Entry = Literal["catalog/adoptions", "catalog/overrides"]
_AREAS = {
    "vocabulary": "vocabulary_adoption",
    "languages": "language_adoption",
    "aliases": "search_alias_adoption",
    "symbols": "text_symbol_adoption",
    "rules-names": "rules_name_adoption",
    "routes": "route_override_adoption",
    "defaults": "default_printing_adoption",
}


def ordered(values: tuple[RecordData, ...]) -> None:
    """Reject duplicate references without imposing authored array ordering."""
    keys = [canonical(item.model_dump(mode="json")) for item in values]
    if len(keys) != len(set(keys)):
        raise ValueError("Catalog array must be unique")


@dataclass(frozen=True)
class LoadedShard:
    path: str
    exact: bytes
    content_hash: str
    content: bytes

    def envelope(self) -> Shard | DisplayShard:
        """Detach typed values from the loaded bytes."""
        model = Shard if self.path.startswith("catalog/adoptions/") else DisplayShard
        return model.model_validate_json(self.content)


@dataclass(frozen=True)
class AdoptionSnapshot:
    entry: Entry
    shards: tuple[LoadedShard, ...]

    def current_records(self) -> tuple[Record, ...]:
        """Sort current selections without revision or decision layers."""
        return tuple(
            sorted(
                (r for s in self.shards for r in s.envelope().records),
                key=lambda r: r.record_key,
            )
        )


def load_adoptions(root: Path, *, entry: Entry) -> AdoptionSnapshot:
    """Check current value keys and kinds across the complete enabled entry."""
    if entry not in {"catalog/adoptions", "catalog/overrides"}:
        raise ValueError("Unknown adoption entry")
    field = "format"
    areas = (
        ("vocabulary", "languages", "aliases", "symbols", "rules-names")
        if entry == "catalog/adoptions"
        else ("routes", "defaults")
    )
    require_directory(root, root / entry)
    paths = tuple(entry + "/" + area for area in areas)
    inputs = _inputs(root, paths)
    if not any((root / path).is_dir() for path in paths):
        raise ValueError("Adoption entry must contain at least one known data area")
    loaded = []
    for name, exact, encoded in inputs:
        content = object_value(parse(encoded))
        if type(content.get(field)) is not int or content[field] != CURRENT_FORMAT:
            raise ValueError("Catalog format must be integer two")
        try:
            shard = (
                Shard if entry == "catalog/adoptions" else DisplayShard
            ).model_validate_json(canonical(content))
        except ValidationError:
            raise ValueError("Invalid current catalog fields") from None
        for record in shard.records:
            if record.kind != _AREAS[Path(name).relative_to(entry).parts[0]]:
                raise ValueError("Current catalog area mismatch")
        loaded.append(LoadedShard(name, exact, digest(encoded), encoded))
    snapshot = AdoptionSnapshot(entry, tuple(loaded))
    records = snapshot.current_records()
    keys = [record.record_key for record in records]
    if len(keys) != len(set(keys)):
        raise ValueError("Duplicate current catalog selection key")
    _values(records)
    return snapshot


def _inputs(root: Path, paths: tuple[str, ...]) -> list[tuple[str, bytes, bytes]]:
    inputs: list[tuple[str, bytes, bytes]] = []
    for area in paths:
        directory = root / area
        check_path(root, directory)
        if not directory.exists():
            continue
        require_directory(root, directory)
        for path in sorted(directory.iterdir()):
            # A nested directory is a stale deeper layout, so it must not read as empty.
            if path.is_dir():
                raise ValueError("Catalog area must hold shards directly")
            if re.fullmatch(r"[0-9]{3,}\.yaml", path.name) is not None:
                exact, encoded = read(path, root=root)
                inputs.append((path.relative_to(root).as_posix(), exact, encoded))
    return inputs


def _values(records: tuple[Record, ...]) -> None:
    symbols: dict[str, str] = {}
    normalizers: set[bytes] = set()
    for record in records:
        ordered(record.data.evidence)
        if (
            isinstance(record, SymbolRecord)
            and (value := record.data.value) is not None
        ) and (
            symbols.setdefault(value.code, record.data.subject.id)
            != record.data.subject.id
        ):
            raise ValueError("Symbol code/id must be one-to-one")
        if isinstance(record, AliasRecord) and (alias := record.data.value) is not None:
            normalizers.add(canonical(alias.normalizer.model_dump(mode="json")))
        if (
            isinstance(record, DefaultRecord)
            and (default := record.data.value) is not None
            and len(default.candidates) != len(set(default.candidates))
        ):
            raise ValueError("Default candidates must be unique")
    if len(normalizers) > 1:
        raise ValueError("Effective aliases cannot mix normalizers")
