"""Project admitted JP inputs to the engine's temporary legacy test interface."""

from dataclasses import asdict
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical
from sve_carddb.domains.parsed_cards.batches import selection

if TYPE_CHECKING:
    from sve_carddb.domains.parsed_cards.storage import Catalog


def project(catalog: Catalog, batch_id: str) -> bytes:
    """Keep every current printing and face; authored effects remain engine inputs."""
    batch = selection(catalog, batch_id)
    if batch.region != "jp":
        raise ValueError("Legacy engine test input requires a JP batch")
    current = {version for version, active in batch.versions.items() if active}
    records: dict[str, bytes] = {}
    seen: set[str] = set()
    for pair in catalog.files:
        if pair.region != "jp":
            continue
        for card in pair.cards:
            for value in card.observations:
                version = value.evidence.descriptor.id
                if version not in current or version in seen:
                    continue
                seen.add(version)
                record = value.document.record
                if record.number in records:
                    raise ValueError("Ambiguous current engine printing")
                records[record.number] = canonical(
                    {
                        "number": record.number,
                        "faces": [asdict(f) for f in record.faces],
                    }
                )
    if seen != current or not records:
        raise ValueError("Engine test selection is incomplete or empty")
    return b"\n".join(records[number] for number in sorted(records)) + b"\n"
