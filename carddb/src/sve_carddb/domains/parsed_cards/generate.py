"""Produce reviewed candidates from sealed SVE sources without writing archives."""

from collections import defaultdict
from typing import TYPE_CHECKING

from sve_carddb.core.json import digest
from sve_carddb.domains.card_extras.archive import card_number
from sve_carddb.domains.parsed_cards.batches import parts
from sve_carddb.domains.parsed_cards.codec import encode
from sve_carddb.domains.parsed_cards.filing import file_path, number_range
from sve_carddb.domains.parsed_cards.models import (
    MAX_BYTES,
    RECIPE,
    BatchPartPin,
    BatchPin,
    BatchSelection,
    Card,
    FilePair,
    FilePin,
    Index,
    ParsedObservation,
)
from sve_carddb.domains.parsed_cards.projection import extract
from sve_carddb.domains.parsed_cards.serialization import separate
from sve_carddb.domains.parsed_cards.storage import Catalog

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping

    from sve_carddb.core.regions import Region
    from sve_carddb.ingest.archive.frozen_sources import FrozenSources


def assemble(
    observations: Iterable[ParsedObservation],
    filing: Mapping[tuple[Region, str], str],
    batches: Iterable[BatchSelection],
) -> Catalog:
    """Group complete histories using the explicit family and fixed numeric ranges."""
    groups: dict[tuple[Region, str, str], dict[str, list[ParsedObservation]]] = (
        defaultdict(lambda: defaultdict(list))
    )
    versions: set[str] = set()
    for item in observations:
        version = item.evidence.descriptor.id
        if version in versions:
            raise ValueError("Duplicate parsed source version")
        versions.add(version)
        region, number = item.document.region, item.document.record.number
        family = filing.get((region, number))
        if family is None:
            raise ValueError("Missing explicit parsed card filing")
        groups[region, family, number_range(number, region)][number].append(item)
    members: dict[str, tuple[FilePair, FilePin]] = {}
    for (region, family, interval), cards in groups.items():
        pair = FilePair(
            region=region,
            family=family,
            range=interval,
            cards=tuple(
                Card(
                    card_no=number,
                    observations=tuple(
                        sorted(items, key=lambda o: o.evidence.descriptor.id)
                    ),
                )
                for number, items in sorted(cards.items())
            ),
        )
        cards_file, evidence_file = separate(pair)
        raw, evidence_raw = encode(cards_file), encode(evidence_file)
        if max(len(raw), len(evidence_raw)) > MAX_BYTES:
            raise ValueError("Parsed file exceeds 1 MiB; review the format contract")
        path = file_path(region, family, pair.cards[0].card_no)
        members[path] = (
            pair,
            FilePin(
                path=path,
                sha256=digest(raw),
                bytes=len(raw),
                evidence_sha256=digest(evidence_raw),
                evidence_bytes=len(evidence_raw),
                cards=len(pair.cards),
                observations=sum(len(c.observations) for c in pair.cards),
            ),
        )
    ordered = [members[k] for k in sorted(members)]
    selections = tuple(sorted(batches, key=lambda b: b.id))
    index = Index(
        files=tuple(p for _, p in ordered),
        batches=tuple(_batch_pin(b) for b in selections),
    )
    result = Catalog(
        index=index, files=tuple(s for s, _ in ordered), batches=selections
    )
    result.validate_batches()
    return result


def generate(
    sources: Iterable[tuple[Region, FrozenSources]],
    filing: Mapping[tuple[Region, str], str],
    *,
    producer_revision: str,
    previous: Catalog | None = None,
) -> Catalog:
    """Append source versions and independent batch selections without rewriting old files."""
    observations = {} if previous is None else previous.observations()
    batches = {} if previous is None else {b.id: b for b in previous.batches}
    effective = _filing(filing, previous)
    for region, source_batch in sources:
        inventory = source_batch.inventory
        if inventory.missing or inventory.history_gaps:
            raise ValueError("Cannot admit an incomplete source batch")
        current = {entry.source_version_id for entry in inventory.current}
        selection = BatchSelection(
            id=source_batch.batch_id,
            region=region,
            versions={
                entry.source_version_id: entry.source_version_id in current
                for entry in sorted(
                    inventory.entries, key=lambda e: e.source_version_id
                )
            },
        )
        if selection.id in batches and batches[selection.id] != selection:
            raise ValueError("Source batch changed under an existing pin")
        batches[selection.id] = selection
        for entry in inventory.entries:
            source, raw, descriptor = source_batch.read(
                entry.source_version_id, parser_version=RECIPE
            )
            number = card_number(descriptor.url, region)
            if (
                descriptor.provider != region
                or descriptor.kind != "card"
                or number is None
            ):
                raise ValueError(
                    "Parsed batch must contain only same-region SVE card pages"
                )
            if (region, number) not in effective:
                raise ValueError("Missing explicit parsed card filing")
            item = extract(
                raw,
                source,
                descriptor,
                region=region,
                producer_revision=producer_revision,
            )
            old = observations.get(descriptor.id)
            if old is not None:
                # A producer commit change alone must not rewrite previously reviewed results.
                same_producer = item.model_copy(
                    update={"producer_revision": old.producer_revision}
                )
                if same_producer.calculated_hash() != old.content_hash:
                    raise ValueError(
                        "Reparsing changed an existing recipe; publish a new recipe"
                    )
                item = old
            observations[descriptor.id] = item
    result = assemble(observations.values(), effective, batches.values())
    if previous is not None:
        result.validate_update(previous)
    return result


def _batch_pin(batch: BatchSelection) -> BatchPin:
    return BatchPin(
        id=batch.id,
        parts=tuple(
            BatchPartPin(sha256=digest(raw), bytes=len(raw))
            for part in parts(batch)
            for raw in (encode(part),)
        ),
    )


def _filing(
    filing: Mapping[tuple[Region, str], str], previous: Catalog | None
) -> dict[tuple[Region, str], str]:
    effective = {} if previous is None else previous.filing()
    for key, family in filing.items():
        if key in effective and effective[key] != family:
            raise ValueError("Parsed request conflicts with existing filing")
        effective[key] = family
    return effective
