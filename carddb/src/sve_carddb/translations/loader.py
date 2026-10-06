"""Validate the complete current glossary closure before projection."""

import re
from dataclasses import dataclass
from functools import cached_property
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError

from sve_carddb.registry.records import RecordData
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import canonical, digest, parse
from sve_carddb.translations.current import records as current_records
from sve_carddb.translations.current import validate as validate_current
from sve_carddb.translations.current_models import Shard as CurrentShard
from sve_carddb.translations.models import Index

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.translations.current_models import Record as CurrentRecord


def _model[T: RecordData](model: type[T], value: JsonValue) -> T:
    try:
        return model.model_validate_json(canonical(value))
    except ValidationError as error:
        location = ".".join(
            str(part) for part in error.errors(include_input=False)[0]["loc"]
        )
        raise ValueError(
            "Invalid translation authored fields at " + (location or "root")
        ) from None


@dataclass(frozen=True)
class Snapshot:
    index: bytes
    shards: tuple[tuple[str, bytes, bytes], ...]
    closure: tuple[tuple[str, bytes, bytes], ...] = ()

    @cached_property
    def _current_values(self) -> tuple[CurrentRecord, ...]:
        """Frozen record models can be shared within this exact byte snapshot."""
        return current_records(self)

    def current_records(self) -> tuple[CurrentRecord, ...]:
        """Expose cached detached current values to input consumers."""
        return self._current_values

    def pins(self) -> dict[str, JsonValue]:
        """Distinguish exact YAML inputs from canonical membership hashes."""
        return {
            "index_hash": digest(self.index),
            "shards": [
                {
                    "path": p,
                    "exact_hash": digest(exact),
                    "canonical_hash": digest(content),
                }
                for p, exact, content in (self.closure or self.shards)
            ],
        }


def validate_snapshot(snapshot: Snapshot) -> None:
    """Validate glossary and name overrides in a separately verified full closure."""
    for path, _, content in snapshot.shards:
        match = re.fullmatch(
            r"translations/(glossary|overrides)/([A-Za-z0-9_-]+)/[0-9]{3,}\.yaml", path
        )
        if match is None:
            raise ValueError("Glossary snapshot contains an unsupported shard path")
        current = _model(CurrentShard, parse(content))
        keys = [r.record_key for r in current.records]
        if keys != sorted(set(keys)):
            raise ValueError("Current translation records must be sorted and unique")
        for record in current.records:
            is_override = record.kind in {"context_assignment", "card_name_concept"}
            if is_override != (match[1] == "overrides"):
                raise ValueError("Translation record is in the wrong authored area")
    validate_current(snapshot)


def _safe(path: Path) -> None:
    if any(part.is_symlink() for part in (path, *path.parents)) or not path.is_file():
        raise ValueError("Missing or symlink translation input")


def load_glossary(root: Path) -> Snapshot:
    """Verify the full closure and project only glossary and name override records."""
    path = root / "translations/index.yaml"
    _safe(path)
    index = _model(Index, read_yaml(path))
    indexed = index.includes
    present = set()
    for file in path.parent.rglob("*"):
        if file.is_symlink():
            raise ValueError("Symlink translation input")
        if not file.is_dir() and file != path:
            present.add(file.relative_to(root).as_posix())
    if present != set(indexed):
        raise ValueError("Translation indexed file closure differs from disk")
    shards = []
    closure = []
    for name, checksum in sorted(indexed.items()):
        match = re.fullmatch(
            r"translations/(glossary|overrides|templates)/([A-Za-z0-9_-]+)/([0-9]{3,})\.yaml",
            name,
        )
        if match is None:
            raise ValueError("Unsafe or unsupported translation include")
        file = root / name
        _safe(file)
        content = canonical(read_yaml(file))
        if digest(content) != checksum:
            raise ValueError("Translation shard hash mismatch")
        closure.append((name, file.read_bytes(), content))
        if match[1] == "templates":
            _template_input(name, content)
            continue
        shards.append((name, file.read_bytes(), content))
    snapshot = Snapshot(path.read_bytes(), tuple(shards), tuple(closure))
    validate_snapshot(snapshot)
    return snapshot


def _template_input(path: str, content: bytes) -> None:
    """Foreign current envelopes are checked without reconstructing source pages."""
    from sve_carddb.template_translations.current import validate_foreign  # ruff: ignore[import-outside-top-level] -- shared foreign validation remains source free

    validate_foreign(path, content)
