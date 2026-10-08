"""Validate the complete current glossary closure before projection."""

import re
from dataclasses import dataclass
from functools import cached_property
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError

from sve_carddb.authored_files import shards
from sve_carddb.registry.records import RecordData
from sve_carddb.snapshot.values import canonical, parse
from sve_carddb.translations.current import records as current_records
from sve_carddb.translations.current import validate as validate_current
from sve_carddb.translations.current_models import Shard as CurrentShard

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
    shards: tuple[tuple[str, bytes, bytes], ...]
    closure: tuple[tuple[str, bytes, bytes], ...] = ()

    @cached_property
    def _current_values(self) -> tuple[CurrentRecord, ...]:
        """Frozen record models can be shared within this exact byte snapshot."""
        return current_records(self)

    def current_records(self) -> tuple[CurrentRecord, ...]:
        """Expose cached detached current values to input consumers."""
        return self._current_values


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
        if len(keys) != len(set(keys)):
            raise ValueError("Current translation records must be unique")
        for record in current.records:
            is_override = record.kind in {"context_assignment", "card_name_concept"}
            if is_override != (match[1] == "overrides"):
                raise ValueError("Translation record is in the wrong authored area")
    validate_current(snapshot)


def load_glossary(root: Path) -> Snapshot:
    """Read current glossary, overrides and templates once from fixed data areas."""
    closure = shards(
        root,
        ("translations/glossary", "translations/overrides", "translations/templates"),
    )
    selected = []
    for name, exact, content in closure:
        if name.startswith("translations/templates/"):
            _template_input(name, content)
        else:
            selected.append((name, exact, content))
    snapshot = Snapshot(tuple(selected), closure)
    validate_snapshot(snapshot)
    return snapshot


def _template_input(path: str, content: bytes) -> None:
    """Foreign current envelopes are checked without reconstructing source pages."""
    from sve_carddb.template_translations.current import validate_foreign  # ruff: ignore[import-outside-top-level] -- shared foreign validation remains source free

    validate_foreign(path, content)
