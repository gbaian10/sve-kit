"""Complete registry input, validated before any regional build projection."""

from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING

from pydantic import ValidationError

from sve_carddb.registry.inputs import canonical
from sve_carddb.registry.records import (
    DATA_MODELS,
    ArtData,
    EnglishPrintingData,
    PrintingData,
    RelatedData,
)
from sve_carddb.registry.storage import Entry, RegistryFiles, read_registry_files
from sve_carddb.registry.validate import check_cursors, validate

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.core.models import RecordData


@dataclass(frozen=True)
class RegistryRecord:
    record_key: str
    kind: str
    owner: str
    data: RecordData
    shard_path: str
    content: bytes

    def entry(self) -> Entry:
        """Return the original record as a detached legacy API value."""
        return Entry.model_validate_json(self.content)


@dataclass(frozen=True)
class RegistrySnapshot:
    files: RegistryFiles
    records: Mapping[str, RegistryRecord]


def load_registry(root: Path) -> RegistrySnapshot:
    """Validate all regional shards without allocating, filtering or writing data.

    The caller must supply a stable authored checkout. This checks historic
    evidence consistency, not freshness against raw sources or release readiness.
    """
    if not (root / "ids" / "index.yaml").is_file():
        raise ValueError("Build input requires an existing registry index")
    files = read_registry_files(root)
    records: dict[str, RegistryRecord] = {}
    entries: list[Entry] = []
    for loaded in files.shards:
        shard = loaded.envelope()
        entries.extend(shard.records)
        for entry in shard.records:
            records[entry.record_key] = RegistryRecord(
                record_key=entry.record_key,
                kind=entry.kind,
                owner=entry.owner,
                data=_data(entry),
                shard_path=loaded.path,
                content=canonical(entry.model_dump(mode="json", round_trip=True)),
            )
    validate(entries)
    check_cursors(files.index().next_int_id, entries)
    _evidence(records)
    return RegistrySnapshot(files, MappingProxyType(records))


def _data(entry: Entry) -> RecordData:
    model = DATA_MODELS[entry.kind]
    if entry.kind == "printing" and entry.data.get("region") == "en":
        model = EnglishPrintingData
    try:
        return model.model_validate_json(canonical(entry.data))
    except ValidationError:
        # Pydantic's default error text includes imported content.
        raise ValueError(f"Invalid registry data: {entry.record_key}") from None


def _evidence(records: dict[str, RegistryRecord]) -> None:
    printings = {
        record.data.id: record.data
        for record in records.values()
        if isinstance(record.data, PrintingData)
    }
    jp = {
        printing.card_no: printing
        for printing in printings.values()
        if printing.region == "jp"
    }
    pairs: set[tuple[str, str]] = set()
    for record in records.values():
        data = record.data
        if isinstance(data, PrintingData):
            _printing_evidence(data, jp)
        elif isinstance(data, ArtData):
            observed = {printings[use.printing_id].observation for use in data.uses}
            if data.observation not in observed or len(set(data.uses)) != len(
                data.uses
            ):
                raise ValueError("Art observation or reviewed uses disagree")
        elif isinstance(data, RelatedData):
            pair = data.from_card_id, data.to_card_id
            if (pair[1], pair[0]) in pairs:
                raise ValueError("Reverse reskin relationship")
            pairs.add(pair)


def _printing_evidence(data: PrintingData, jp: dict[str, PrintingData]) -> None:
    if (data.observation.region, data.observation.card_no) != (
        data.region,
        data.card_no,
    ):
        raise ValueError("Printing observation disagrees with its identity")
    if not isinstance(data, EnglishPrintingData):
        return
    review = data.cross_region_review
    if not review.checked:
        raise ValueError("English printing lacks explicit checked review")
    target = review.target_jp_card_no
    if target is None and review.target_observation is not None:
        raise ValueError("English-only review must not invent a JP observation")
    if target is not None and (
        target not in jp or review.target_observation != jp[target].observation
    ):
        raise ValueError("Cross-region target observation disagrees with registry")
