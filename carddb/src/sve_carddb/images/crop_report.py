"""Report adopted crops and verified reprint candidates without inheriting boxes."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.images.crops import image_source_key

if TYPE_CHECKING:
    from sve_carddb.core.regions import Region
    from sve_carddb.images.assets import ImageBuild
    from sve_carddb.images.crops import CropRecord, ImageCrops


@dataclass(frozen=True)
class CropBinding:
    """Carry adopted ownership only for a face with a verified regional image."""

    card_id: str
    printing_id: str
    face_id: str
    region: Region
    card_no: str
    source_url: str


def _label(record: CropRecord) -> dict[str, JsonValue]:
    return {
        "source_key": record.source_key,
        "source_sha256": record.source_sha256,
        "region": record.region,
        "card_no": record.card_no,
    }


def crop_report(
    crops: ImageCrops,
    images: ImageBuild,
    references: tuple[CropBinding, ...],
) -> dict[str, JsonValue]:
    """Use adopted card/face ownership only for non-blocking diagnostics."""
    by_url = {
        (item.region, item.source.url): crops.records.get(
            (image_source_key(item.region, item.source.url), item.source.sha256[7:])
        )
        for item in images.images
    }
    applied = {record.key for record in by_url.values() if record is not None}
    covered: dict[tuple[str, str], set[str]] = {}
    for ref in references:
        if by_url.get((ref.region, ref.source_url)) is not None:
            covered.setdefault((ref.card_id, ref.face_id), set()).add(ref.printing_id)
    warnings: list[JsonValue] = []
    annotations: list[JsonValue] = []
    for ref in references:
        record = by_url.get((ref.region, ref.source_url))
        if record is not None and (record.region, record.card_no) != (
            ref.region,
            ref.card_no,
        ):
            annotations.append(
                _label(record)
                | {"printing_id": ref.printing_id, "face_id": ref.face_id}
            )
        key = ref.card_id, ref.face_id
        other = covered.get(key, set()) - {ref.printing_id}
        if record is None and other:
            warnings.append(
                {
                    "card_id": key[0],
                    "face_id": key[1],
                    "printing_id": ref.printing_id,
                    "source_key": image_source_key(ref.region, ref.source_url),
                    "other_printing_ids": list[JsonValue](sorted(other)),
                }
            )
    return {
        "applied_source_images": len(applied),
        "unused": [
            _label(record)
            for key, record in sorted(crops.records.items())
            if key not in applied
        ],
        "annotation_mismatches": annotations,
        "reprint_candidates": warnings,
    }
