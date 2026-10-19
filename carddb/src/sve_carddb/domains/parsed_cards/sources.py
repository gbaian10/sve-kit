"""Adapt reviewed parsed inputs to existing frozen-source consumers without raw I/O."""

import re
from pathlib import PurePosixPath
from typing import TYPE_CHECKING, override

from sve_carddb.core.json import digest
from sve_carddb.domains.parsed_cards import batches
from sve_carddb.domains.parsed_cards.projection import replay
from sve_carddb.ingest.archive.frozen_sources import FrozenBatches, FrozenSources
from sve_carddb.ingest.archive.source_archive import ArchiveError, Current, Inventory
from sve_carddb.ingest.archive.store import resolve_within

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping
    from pathlib import Path

    from sve_carddb.core.provenance import Source
    from sve_carddb.domains.parsed_cards.models import ParsedObservation
    from sve_carddb.domains.parsed_cards.storage import Catalog
    from sve_carddb.domains.physical_sources import PhysicalPage
    from sve_carddb.ingest.archive.source_archive import Descriptor


class ParsedSources(FrozenSources):
    def __init__(
        self, root: Path, store_id: str, batch_id: str, catalog: Catalog
    ) -> None:
        self.selection = batches.selection(catalog, batch_id)
        self.catalog = catalog
        self.root, self.store_id, self.batch_id = root, store_id, batch_id
        self._values = {
            value.evidence.descriptor.id: value
            for pair in catalog.files
            for card in pair.cards
            for value in card.observations
            if value.evidence.descriptor.id in self.selection.versions
        }
        self._pages: dict[str, PhysicalPage] = {}

    @property
    @override
    def version_ids(self) -> frozenset[str]:
        """Expose only source versions selected in this reviewed batch."""
        return frozenset(self.selection.versions)

    @property
    @override
    def current_versions(self) -> tuple[Current, ...]:
        """Reproduce the original URL ordering without recreating an inventory."""
        return tuple(
            sorted(
                (
                    Current(url=self.descriptor(version).url, source_version_id=version)
                    for version, current in self.selection.versions.items()
                    if current
                ),
                key=lambda item: item.url,
            )
        )

    def _observation(self, version: str) -> ParsedObservation:
        value = self._values.get(version)
        if value is None:
            raise ArchiveError("Source version is absent from pinned batch")
        return value

    @override
    def descriptor(self, version: str) -> Descriptor:
        """Return only descriptors belonging to this verified original batch."""
        return self._observation(version).evidence.descriptor

    @override
    def metadata(
        self, version: str, *, parser_version: str
    ) -> tuple[Source, Descriptor]:
        """Restore the same provenance fields without archive files or media reads."""
        value = self._observation(version)
        return batches.source(
            self.catalog,
            value,
            batch_id=self.batch_id,
            store_id=self.store_id,
            parser_version=parser_version,
        ), value.evidence.descriptor

    def physical(self, version: str, *, parser_version: str) -> PhysicalPage:
        """Share the first physical projection exactly as the archive reader does."""
        if version not in self._pages:
            source, _ = self.metadata(version, parser_version=parser_version)
            self._pages[version] = replay(self._values[version], source)
        return self._pages[version]

    @override
    def read(
        self, version: str, *, parser_version: str
    ) -> tuple[Source, bytes, Descriptor]:
        """Reject raw-only consumers rather than reaching an implicit HTML fallback."""
        raise ValueError("Parsed sources do not expose raw HTML")

    @override
    def projection[T](
        self,
        version: str,
        *,
        parser_version: str,
        extract: Callable[[Source, bytes, Descriptor], T],
    ) -> T:
        """Only the explicit physical-page adapter can consume portable projections."""
        raise ValueError("Parsed sources require a physical-page projection")


class ParsedBatches(FrozenBatches):
    def __init__(self, catalog: Catalog) -> None:
        super().__init__()
        self.catalog = catalog
        self._known = {b.id for b in catalog.index.batches}

    @override
    def batch(self, root: Path, store_id: str, batch_id: str) -> FrozenSources:
        """Use reviewed SVE inputs while leaving independent media/API batches intact."""
        key = root.resolve(), store_id, batch_id
        if key in self.batches:
            return self.batches[key]
        if batch_id not in self._known:
            # Inspect the hash-bound scope before any raw verification can open SVE HTML.
            if re.fullmatch(r"sha256:[0-9a-f]{64}", batch_id) is None:
                raise ValueError("Invalid configured source batch ID")
            raw = resolve_within(
                root, PurePosixPath(f"batches/{batch_id[7:]}/inventory.json")
            ).read_bytes()
            if digest(raw) != batch_id:
                raise ValueError("Unparsed inventory hash mismatch")
            inventory = Inventory.model_validate_json(raw)
            if any(
                s.provider in {"en", "jp"} and s.kind == "card" for s in inventory.scope
            ):
                raise ValueError("SVE card batch is absent from reviewed parsed inputs")
            return super().batch(root, store_id, batch_id)
        self.batches[key] = ParsedSources(root, store_id, batch_id, self.catalog)
        return self.batches[key]

    @override
    def configured(self, stores: Mapping[str, Path], batch_id: str) -> FrozenSources:
        """Use the explicit runtime store binding; public inputs cannot prove ownership."""
        if batch_id not in self._known:
            return super().configured(stores, batch_id)
        if len(stores) != 1:
            raise ValueError("Parsed batch requires exactly one explicit store binding")
        name, root = next(iter(stores.items()))
        return self.batch(root, name, batch_id)
