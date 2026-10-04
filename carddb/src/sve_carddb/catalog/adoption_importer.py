"""Immutable catalog input pins shared by the current offline composer."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.catalog.adoption_loader import AdoptionSnapshot, load_adoptions
from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.translations.importer import Inputs as TranslationInputs

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.catalog.adoption_loader import Entry


@dataclass(frozen=True)
class AdoptionInputs:
    root: Path
    repository: Path
    authored_revision: str
    entries: tuple[Entry, ...]
    include_translations: bool = False

    def translation_inputs(self) -> TranslationInputs | None:
        """Enable the complete existing entry, never a filtered glossary subset."""
        if type(self.include_translations) is not bool:
            raise ValueError("Translation composition flag must be boolean")
        if not self.include_translations:
            return None
        return TranslationInputs(self.root, self.repository, self.authored_revision)

    def load(self) -> tuple[AdoptionSnapshot, ...]:
        """Reload immutable bytes instead of accepting a caller-made approval object."""
        if not re.fullmatch(r"[0-9a-f]{40}", self.authored_revision):
            raise ValueError("Adoption authored revision must be a full Git SHA")
        if not self.entries or self.entries != tuple(sorted(set(self.entries))):
            raise ValueError(
                "Adoption entries must be explicitly enabled, sorted and unique"
            )
        snapshots = tuple(
            load_adoptions(self.root, entry=entry) for entry in self.entries
        )
        repository = PinnedRepository(self.repository)
        for snapshot in snapshots:
            files = [(snapshot.entry + "/index.yaml", snapshot.index_exact)]
            files.extend((s.path, s.exact) for s in snapshot.shards)
            for name, content in files:
                if (
                    repository.read(self.authored_revision, "authored/" + name)
                    != content
                ):
                    raise ValueError(
                        "Adoption bytes differ from immutable authored revision"
                    )
        return snapshots

    def configuration(self) -> dict[str, JsonValue]:
        """Pin both enabled entries, complete byte inventories, and the authored commit."""
        snapshots = self.load()
        translation = self.translation_inputs()
        return {
            **({} if translation is None else translation.configuration()),
            "catalog_adoptions": {
                "include_translations": self.include_translations,
                "authored_revision": self.authored_revision,
                "inputs": [s.pins() for s in snapshots],
                "current": [
                    r.model_dump(mode="json")
                    for snapshot in snapshots
                    for r in snapshot.current_records()
                ],
                "effective": [
                    {
                        "record_key": r.record_key,
                        "record_hash": digest(canonical(r.model_dump(mode="json"))),
                        "decision_id": decision,
                        "dependencies": [
                            d.model_dump(mode="json") for d in r.data.dependencies
                        ],
                    }
                    for snapshot in snapshots
                    for r, decision in snapshot.effective()
                ],
            },
        }
