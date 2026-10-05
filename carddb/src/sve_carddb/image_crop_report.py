"""Report adopted crops and verified reprint candidates without inheriting boxes."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.image_crops import image_source_key
from sve_carddb.snapshot.project.source import Source
from sve_carddb.snapshot.values import string

if TYPE_CHECKING:
    from sve_carddb.build_db import Database
    from sve_carddb.image_assets import ImageBuild, ImageReference
    from sve_carddb.image_crops import CropRecord, ImageCrops


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
    references: tuple[ImageReference, ...],
    db: Database,
) -> dict[str, JsonValue]:
    """Use effective DB card/face ownership only for non-blocking diagnostics."""
    by_url = {
        item.source.url: crops.records.get(
            (image_source_key(item.region, item.source.url), item.source.sha256[7:])
        )
        for item in images.images
    }
    applied = {record.key for record in by_url.values() if record is not None}
    printings = {
        string(row["id"]): string(row["card_id"])
        for row in Source(db).rows("printing", "id,card_id")
    }
    covered: dict[tuple[str, str], set[str]] = {}
    for ref in references:
        if by_url.get(ref.source_url) is not None:
            covered.setdefault((printings[ref.printing_id], ref.face_id), set()).add(
                ref.printing_id
            )
    warnings: list[JsonValue] = []
    annotations: list[JsonValue] = []
    for ref in references:
        record = by_url.get(ref.source_url)
        if record is not None and (record.region, record.card_no) != (
            ref.region,
            ref.card_no,
        ):
            annotations.append(
                _label(record)
                | {"printing_id": ref.printing_id, "face_id": ref.face_id}
            )
        key = printings[ref.printing_id], ref.face_id
        other = covered.get(key, set()) - {ref.printing_id}
        if record is None and other:
            # A missing regional pipeline must not be reported as a verified binding.
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
