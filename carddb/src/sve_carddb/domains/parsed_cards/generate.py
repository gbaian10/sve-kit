"""Produce reviewed candidates from sealed SVE sources without writing archives."""

from collections import defaultdict
from typing import TYPE_CHECKING

from sve_carddb.core.json import digest
from sve_carddb.domains.card_extras.archive import card_number
from sve_carddb.domains.parsed_cards.codec import encode
from sve_carddb.domains.parsed_cards.filing import band as number_band
from sve_carddb.domains.parsed_cards.filing import shard_path
from sve_carddb.domains.parsed_cards.models import (
    MAX_BYTES,
    RECIPE,
    BatchPin,
    Card,
    Index,
    Membership,
    ParsedObservation,
    Shard,
    ShardPin,
    SplitFamily,
)
from sve_carddb.domains.parsed_cards.projection import extract
from sve_carddb.domains.parsed_cards.storage import Catalog

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping

    from sve_carddb.core.regions import Region
    from sve_carddb.ingest.archive.frozen_sources import FrozenSources


def assemble(
    observations: Iterable[ParsedObservation],
    filing: Mapping[tuple[Region, str], str],
    batches: Iterable[BatchPin],
    *,
    split_families: tuple[SplitFamily, ...] = (),
) -> Catalog:
    """Group complete histories using the explicit family and fixed numeric ranges."""
    splits = {(s.region, s.family) for s in split_families}
    groups: dict[tuple[Region, str, str | None], dict[str, list[ParsedObservation]]] = (
        defaultdict(lambda: defaultdict(list))
    )
    for item in observations:
        region, number = item.document.region, item.document.record.number
        family = filing[region, number]
        band = number_band(number, region) if (region, family) in splits else None
        groups[region, family, band][number].append(item)
    members: dict[str, tuple[Shard, ShardPin]] = {}
    for (region, family, band), cards in groups.items():
        shard = Shard(
            region=region,
            family=family,
            band=band,
            cards=tuple(
                Card(
                    card_no=number,
                    observations=tuple(
                        sorted(
                            items, key=lambda o: (o.evidence.descriptor.id, o.recipe)
                        )
                    ),
                )
                for number, items in sorted(cards.items())
            ),
        )
        raw = encode(shard)
        if len(raw) > MAX_BYTES:
            raise ValueError(
                f"Parsed family exceeds 1 MiB; explicitly split {region}/{family}"
            )
        path = shard_path(region, family, shard.cards[0].card_no, splits)
        members[path] = (
            shard,
            ShardPin(
                path=path,
                sha256=digest(raw),
                bytes=len(raw),
                cards=len(shard.cards),
                observations=sum(len(c.observations) for c in shard.cards),
            ),
        )
    ordered = [members[k] for k in sorted(members)]
    index = Index(
        split_families=split_families,
        shards=tuple(p for _, p in ordered),
        batches=tuple(sorted(batches, key=lambda b: b.id)),
    )
    return Catalog(index=index, shards=tuple(s for s, _ in ordered))


def generate(
    sources: Iterable[tuple[Region, FrozenSources]],
    filing: Mapping[tuple[Region, str], str],
    *,
    producer_revision: str,
    split_families: tuple[SplitFamily, ...] = (),
    previous: Catalog | None = None,
) -> Catalog:
    """Append verified source versions while retaining every reviewed observation."""
    observations = {} if previous is None else previous.observations()
    batches = {} if previous is None else {b.id: b for b in previous.index.batches}
    for region, source_batch in sources:
        inventory = source_batch.inventory
        if inventory.missing or inventory.history_gaps:
            raise ValueError("Cannot admit an incomplete source batch")
        current = {entry.source_version_id for entry in inventory.current}
        pin = BatchPin(
            id=source_batch.batch_id,
            region=region,
            versions=len(inventory.entries),
            current=len(current),
            created_at=inventory.created_at,
            manifest=inventory.manifest,
        )
        if pin.id in batches and batches[pin.id] != pin:
            raise ValueError("Source batch changed under an existing pin")
        batches[pin.id] = pin
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
            if (region, number) not in filing:
                raise ValueError("Missing explicit parsed card filing")
            membership = Membership(
                batch_id=pin.id,
                current=descriptor.id in current,
                receipt_id=entry.receipt_id,
            )
            old = observations.get(descriptor.id)
            memberships = tuple(
                sorted(
                    {membership, *(old.memberships if old else ())},
                    key=lambda m: m.batch_id,
                )
            )
            item = extract(
                raw,
                source,
                descriptor,
                region=region,
                producer_revision=producer_revision,
                memberships=memberships,
            )
            if old is not None:
                # A producer commit change alone must not rewrite previously reviewed results.
                same_producer = item.model_copy(
                    update={"producer_revision": old.producer_revision}
                )
                if same_producer.calculated_hash() != old.content_hash:
                    raise ValueError(
                        "Reparsing changed an existing recipe; publish a new recipe"
                    )
                item = old.model_copy(update={"memberships": memberships})
            observations[descriptor.id] = item
    result = assemble(
        observations.values(), filing, batches.values(), split_families=split_families
    )
    if previous is not None:
        result.validate_update(previous)
    return result
