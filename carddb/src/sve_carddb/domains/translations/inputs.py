"""Validate the complete current glossary closure before projection."""

from dataclasses import dataclass
from functools import cached_property
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.authored import require_directory
from sve_carddb.domains.translations.four_layer_authored import (
    Inputs as FourLayerInputs,
)
from sve_carddb.domains.translations.four_layer_authored import from_files, read_inputs
from sve_carddb.domains.translations.glossary.validate import records as current_records
from sve_carddb.domains.translations.glossary.validate import (
    validate as validate_current,
)

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.translations.glossary.records import Record as CurrentRecord


@dataclass(frozen=True)
class Snapshot:
    shards: tuple[tuple[str, bytes, bytes], ...]
    closure: tuple[tuple[str, bytes, bytes], ...] = ()

    @cached_property
    def four_layer(self) -> FourLayerInputs:
        """All consumers share one strict format-three closure and its source-free checks."""
        if self.closure and self.shards != tuple(
            file
            for file in self.closure
            if file[0].startswith(("translations/glossary/", "translations/overrides/"))
        ):
            raise ValueError(
                "Translation snapshot projections differ from their shared closure"
            )
        return from_files(self.closure or self.shards)

    @cached_property
    def _current_values(self) -> tuple[CurrentRecord, ...]:
        """Frozen record models can be shared within this exact byte snapshot."""
        return current_records(self)

    def current_records(self) -> tuple[CurrentRecord, ...]:
        """Expose cached detached current values to input consumers."""
        return self._current_values


def validate_snapshot(snapshot: Snapshot) -> None:
    """Validate glossary and name overrides in a separately verified full closure."""
    _ = snapshot.four_layer
    validate_current(snapshot)


def load_glossary(root: Path) -> Snapshot:
    """Read current glossary, overrides and templates once from fixed data areas."""
    require_directory(root, root / "translations/glossary")
    loaded = read_inputs(root)
    selected = tuple(
        file
        for file in loaded.files
        if file[0].startswith(("translations/glossary/", "translations/overrides/"))
    )
    snapshot = Snapshot(selected, loaded.files)
    validate_snapshot(snapshot)
    return snapshot


@dataclass(frozen=True)
class Inputs:
    root: Path
    repository: Path
    authored_revision: str

    @cached_property
    def snapshot(self) -> Snapshot:
        """Share one working-tree read across all consumers in this command."""
        return load_glossary(self.root)

    def load(self) -> Snapshot:
        """Reuse current inputs within this command."""
        return self.snapshot

    def configuration(self) -> dict[str, JsonValue]:
        """Describe enabled input areas without locking mutable authored bytes."""
        return {"translation_authored": {"authored_revision": self.authored_revision}}
