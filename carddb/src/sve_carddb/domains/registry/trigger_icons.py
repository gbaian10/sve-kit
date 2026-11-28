"""Reviewed Trigger icon observations from frozen printing images."""

from typing import TYPE_CHECKING, Literal
from urllib.parse import urlsplit

from pydantic import ValidationError

from sve_carddb.core.authored import shards
from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.core.regions import Region
from sve_carddb.domains.registry.records import FaceId, PrintingId
from sve_carddb.ingest.archive.source_archive import read_sealed_inventory

if TYPE_CHECKING:
    from pathlib import Path

AREA = "catalog/trigger-icons"
HOSTS = {"jp": "shadowverse-evolve.com", "en": "en.shadowverse-evolve.com"}


class Observation(RecordData):
    printing_id: PrintingId
    card_no: Text
    face_id: FaceId
    icon: Literal["critical", "draw", "stand", "heal", "none"]
    source_url: Text
    source_sha256: Hash


class IconShard(RecordData):
    format: int
    kind: Literal["trigger_icons_shard"]
    region: Region
    image_batch: Hash
    records: tuple[Observation, ...]


def _image_hashes(
    archive: Path, store_id: str, region: Region, batch: str
) -> dict[str, str]:
    inventory = read_sealed_inventory(archive, batch)
    if [(item.provider, item.kind) for item in inventory.scope] != [(region, "image")]:
        raise ValueError("Trigger icon evidence requires a sealed regional image batch")
    hashes = {
        entry.source_version_id: entry.blob.sha256
        for entry in inventory.entries
        if entry.blob.store_id == store_id
    }
    return {
        entry.url: hashes[entry.source_version_id]
        for entry in inventory.current
        if entry.source_version_id in hashes
    }


def load_trigger_icons(
    authored: Path, *, archive: Path, store_id: str
) -> dict[tuple[str, str], tuple[Region, Observation]]:
    """Validate the review records without opening a crawler cache."""
    result: dict[tuple[str, str], tuple[Region, Observation]] = {}
    batches: dict[tuple[Region, str], dict[str, str]] = {}
    for path, _, content in shards(authored, (AREA,), optional=(AREA,)):
        try:
            shard = IconShard.model_validate_json(content)
        except ValidationError as error:
            details = "; ".join(
                ".".join(map(str, item["loc"])) + ":" + item["type"]
                for item in error.errors(include_input=False, include_context=False)
            )
            raise ValueError(f"Invalid Trigger icon shard {path}: {details}") from None
        if shard.format != 1 or path.rsplit("/", 2)[1] != shard.region:
            raise ValueError(f"Invalid Trigger icon shard identity: {path}")
        pin = (shard.region, shard.image_batch)
        if pin not in batches:
            batches[pin] = _image_hashes(archive, store_id, *pin)
        image_hashes = batches[pin]
        for record in shard.records:
            key = (record.printing_id, record.face_id)
            if key in result:
                raise ValueError(f"Duplicate Trigger icon: {key}")
            url = urlsplit(record.source_url)
            if (
                url.scheme != "https"
                or url.netloc != HOSTS[shard.region]
                or not url.path
            ):
                raise ValueError(f"Invalid Trigger icon image URL: {key}")
            if image_hashes.get(record.source_url) != record.source_sha256:
                raise ValueError(
                    f"Trigger icon image is absent from its sealed batch: {key}"
                )
            result[key] = (shard.region, record)
    return result
