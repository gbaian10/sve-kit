"""Replay approved date-only list evidence without widening generic source readers."""

from dataclasses import asdict
from datetime import datetime
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING
from zoneinfo import ZoneInfo

from pydantic import JsonValue

from sve_carddb.core.json import canonical, digest, object_value, parse
from sve_carddb.core.provenance import (
    ArchivePin,
    DateSource,
    DateSourceUse,
    Source,
    SourceUse,
)
from sve_carddb.core.regions import SourceRegion
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Manifest
from sve_carddb.ingest.archive.source_archive import Receipt
from sve_carddb.ingest.archive.source_import.models import load_index
from sve_carddb.ingest.archive.store import resolve_within
from sve_carddb.parse.pages.restriction_lifts import parse_jp_restriction_lifts
from sve_carddb.parse.pages.restriction_list import parse_restriction_list

if TYPE_CHECKING:
    from sve_carddb.core.regions import Region

PARSER_VERSION = "construction-current-list-v1"


class Lists:
    def __init__(self, stores: dict[str, Path], batch_ids: tuple[str, ...]) -> None:
        self.batches = tuple(
            FrozenSources.configured(stores, batch_id) for batch_id in batch_ids
        )
        self.uses: set[SourceUse | DateSourceUse] = set()
        self._sources: dict[tuple[str, str], Source | DateSource] = {}
        self._results: dict[tuple[str, str], dict[str, JsonValue]] = {}

    def source(self, batch_id: str, version: str) -> Source | DateSource:
        """Recover the original approved observation without synthesizing an instant."""
        key = batch_id, version
        if key in self._sources:
            return self._sources[key]
        matches = [batch for batch in self.batches if batch.batch_id == batch_id]
        if len(matches) != 1:
            raise ValueError("Construction source requires its exact context batch")
        batch = matches[0]
        descriptor = batch.descriptor(version)
        entry = batch.entries[version]
        receipt_path = resolve_within(
            batch.root,
            PurePosixPath("receipts", descriptor.first_receipt_id[7:] + ".json"),
        )
        raw_receipt = receipt_path.read_bytes()
        if digest(raw_receipt) != descriptor.first_receipt_id:
            raise ValueError("Construction observation receipt hash mismatch")
        receipt = Receipt.model_validate_json(raw_receipt)
        if "T" in receipt.resource.last_changed_at:
            source, _ = batch.metadata(version, parser_version=PARSER_VERSION)
            self._sources[key] = source
            return source
        manifest = Manifest.open_snapshot(
            resolve_within(
                batch.root / "batches" / batch_id[7:],
                PurePosixPath(batch.inventory.manifest.path),
            )
        )
        try:
            imported = manifest.source_import_receipt()
            if imported is None or imported.content.date_precision_approval is None:
                raise ValueError(
                    "Date adoption requires an approved isolated source import"
                )
            observations = [
                row
                for row in load_index(imported.index_bytes)
                if row.url == descriptor.url
            ]
            if len(observations) != 1:
                raise ValueError(
                    "Construction source lacks a unique import observation"
                )
            observation = observations[0]
            if (
                observation.fetched_at is not None
                or observation.observed_on != receipt.resource.last_changed_at
                or "sha256:" + observation.sha256 != descriptor.raw_sha256
                or observation.media_type != "text/html"
            ):
                raise ValueError(
                    "Construction date differs from its frozen observation"
                )
            assert observation.observed_timezone is not None
            assert observation.observed_basis is not None
            dated = DateSource(
                id=descriptor.id,
                kind="official_page",
                url=descriptor.url,
                raw_locator=f"{batch.store_id}:{entry.blob.path}",
                sha256=descriptor.raw_sha256,
                fetched_at=None,
                observed_on=observation.observed_on,
                observed_timezone=observation.observed_timezone,
                observed_basis=observation.observed_basis,
                import_receipt_id=imported.receipt_id,
                etag=observation.etag,
                last_modified=observation.last_modified,
                parser_version=PARSER_VERSION,
                archive=ArchivePin(
                    store_id=batch.store_id,
                    batch_id=batch_id,
                    descriptor_sha256=entry.descriptor_sha256,
                    first_receipt_id=descriptor.first_receipt_id,
                ),
            )
        finally:
            manifest.close()
        self._sources[key] = dated
        return dated

    def projection(
        self, batch_id: str, version: str, region: Region
    ) -> dict[str, JsonValue]:
        """Replay the pinned parser against hash-verified regional bytes."""
        key = batch_id, version
        source = self.source(batch_id, version)
        if key not in self._results:
            batch = next(batch for batch in self.batches if batch.batch_id == batch_id)
            descriptor = batch.descriptor(version)
            if descriptor.provider != region or descriptor.kind not in {
                "limit",
                "news",
            }:
                raise ValueError(
                    "Current list evidence must have its regional limit classification"
                )
            raw = resolve_within(
                batch.root, PurePosixPath(batch.entries[version].blob.path)
            ).read_bytes()
            if digest(raw) != source.sha256:
                raise ValueError("Construction raw bytes hash mismatch")
            if descriptor.kind == "news":
                if region != "jp":
                    raise ValueError(
                        "Only explicit JP Standard lift notices are supported"
                    )
                self._results[key] = object_value(
                    parse(
                        canonical(
                            {
                                "region": region,
                                "lifts": [
                                    dict[str, JsonValue](item)
                                    for item in parse_jp_restriction_lifts(raw)
                                ],
                            }
                        )
                    )
                )
                return self._results[key]
            parsed = parse_restriction_list(raw, SourceRegion(region))
            entries: list[JsonValue] = [
                asdict(item)
                | {
                    "effective_from": item.effective_from.isoformat(),
                    "effective_until": None
                    if item.effective_until is None
                    else item.effective_until.isoformat(),
                }
                for item in parsed.entries
            ]
            self._results[key] = object_value(
                parse(
                    canonical(
                        {
                            "region": parsed.region.value,
                            "entries": entries,
                            "excluded_sections": list(parsed.excluded_sections),
                        }
                    )
                )
            )
        if self._results[key]["region"] != region:
            raise ValueError("Construction list cannot cross regions")
        return self._results[key]

    def verify(  # ruff: ignore[complex-structure] -- Keep the complete admission boundary visible.
        self, use: SourceUse | DateSourceUse, region: Region, record_key: str
    ) -> JsonValue:
        """Check the actual pointer result and record the consumed source use."""
        source = self.source(use.source.archive.batch_id, use.source.id)
        if source != use.source:
            raise ValueError(
                "Construction evidence differs from frozen metadata or recipe"
            )
        projection = self.projection(source.archive.batch_id, source.id, region)
        locator = object_value(parse(use.locator.encode()))
        if canonical(locator).decode() != use.locator:
            raise ValueError("Construction locator must be canonical JSON")
        if (
            set(locator)
            != {
                "record_key",
                "field",
                "context_key",
                "number",
                "location",
                "result_hash",
            }
            or locator["record_key"] != record_key
        ):
            raise ValueError("Construction locator fields or record key mismatch")
        if (
            locator["field"] != "value"
            or locator["context_key"] != "standard"
            or locator["number"] is not None
        ):
            raise ValueError("Unsupported current-list locator context")
        location = object_value(locator["location"])
        if (
            set(location) != {"kind", "pointer", "page_numbers"}
            or location["kind"] != "projection_pointer"
            or location["page_numbers"] != []
        ):
            raise ValueError(
                "Current-list HTML requires a projection pointer without PDF pages"
            )
        pointer = location["pointer"]
        value: JsonValue = projection
        if not isinstance(pointer, str) or (pointer and not pointer.startswith("/")):
            raise ValueError("Invalid construction projection pointer")
        for component in pointer.split("/")[1:]:
            if isinstance(value, dict) and component in value:
                value = value[component]
            elif (
                isinstance(value, list)
                and component.isdecimal()
                and str(int(component)) == component
                and int(component) < len(value)
            ):
                value = value[int(component)]
            else:
                raise ValueError("Construction projection pointer is absent")
        if digest(canonical(value)) != locator["result_hash"]:
            raise ValueError("Construction locator result differs from replay")
        self.uses.add(use)
        return value

    def regional_versions(self, region: Region, batch_id: str) -> frozenset[str]:
        """Rebuild the regional source denominator independently of authored choices."""
        return frozenset(
            version
            for batch in self.batches
            if batch.batch_id == batch_id
            for version in batch.version_ids
            if (descriptor := batch.descriptor(version)).provider == region
            and descriptor.kind in {"limit", "news"}
        )


def observed_day(source: Source | DateSource, timezone: str) -> str:
    """Compare an instant in the declared observation zone without losing its precision."""
    if isinstance(source, DateSource):
        if source.observed_timezone != timezone:
            raise ValueError("Observation timezone differs from its date source")
        return source.observed_on
    return (
        datetime.fromisoformat(source.fetched_at)
        .astimezone(ZoneInfo(timezone))
        .date()
        .isoformat()
    )
