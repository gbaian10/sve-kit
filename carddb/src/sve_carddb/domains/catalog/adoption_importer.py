"""Immutable catalog input pins shared by the current offline composer."""

from dataclasses import dataclass
from functools import cached_property
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.domains.catalog.adoption_loader import AdoptionSnapshot, load_adoptions
from sve_carddb.domains.translations.inputs import Inputs as TranslationInputs

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.catalog.adoption_loader import Entry


@dataclass(frozen=True)
class AdoptionInputs:
    root: Path
    repository: Path
    authored_revision: str
    entries: tuple[Entry, ...]
    include_translations: bool = False

    @cached_property
    def translation(self) -> TranslationInputs | None:
        """Reuse current inputs within this command."""
        if type(self.include_translations) is not bool:
            raise ValueError("Translation composition flag must be boolean")
        return (
            TranslationInputs(self.root, self.repository, self.authored_revision)
            if self.include_translations
            else None
        )

    def translation_inputs(self) -> TranslationInputs | None:
        """Reuse current inputs within this command."""
        return self.translation

    @cached_property
    def snapshots(self) -> tuple[AdoptionSnapshot, ...]:
        """Reuse current inputs within this command."""
        if not self.entries or self.entries != tuple(sorted(set(self.entries))):
            raise ValueError(
                "Adoption entries must be explicitly enabled, sorted and unique"
            )
        return tuple(load_adoptions(self.root, entry=entry) for entry in self.entries)

    def load(self) -> tuple[AdoptionSnapshot, ...]:
        """Reuse current inputs within this command."""
        return self.snapshots

    def configuration(self) -> dict[str, JsonValue]:
        """Reuse current inputs within this command."""
        translation = self.translation_inputs()
        return {
            **({} if translation is None else translation.configuration()),
            "catalog_adoptions": {
                "include_translations": self.include_translations,
                "authored_revision": self.authored_revision,
                "entries": list(self.entries),
            },
        }
