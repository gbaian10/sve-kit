"""Report adopted crops and verified reprint candidates without inheriting boxes."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.project.source import Source
from sve_carddb.snapshot.values import canonical, digest, string

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
    by_id = {record.image_id: record for record in crops.records.values()}
    used = {item.result.image_id for item in images.images}
    by_url = {
        item.source.url: by_id.get(item.result.image_id) for item in images.images
    }
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
            "jp",
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
            source_key = digest(
                canonical({"provider": "jp", "kind": "image", "url": ref.source_url})
            )
            warnings.append(
                {
                    "card_id": key[0],
                    "face_id": key[1],
                    "printing_id": ref.printing_id,
                    "source_key": source_key,
                    "other_printing_ids": list[JsonValue](sorted(other)),
                }
            )
    return {
        "applied_source_images": len(used & by_id.keys()),
        "art_webp_review": "pending_coordinator_review",
        "unused": [
            _label(record)
            for key, record in sorted(crops.records.items())
            if record.image_id not in used
        ],
        "annotation_mismatches": annotations,
        "reprint_candidates": warnings,
    }
