"""Public image handoff rejects private fields and invalid publication metadata."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.export.read_api import (
    ExportError,
    ImageInventory,
    ImageObject,
    ImageUpload,
    canonical,
    digest,
    load_image_upload,
    read_image_inventory,
)

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue


def item() -> ImageObject:
    return ImageObject(
        key="images/card_s/123.webp",
        version=7,
        sha256="sha256:" + "a" * 64,
        bytes=100,
    )


def test_content_addressed_public_description_round_trips(tmp_path: Path) -> None:
    value = ImageUpload(
        format="sve-image-upload-v1",
        image_index_sha256="sha256:" + "b" * 64,
        revision=7,
        objects=(item(),),
    )
    raw = value.content()
    path = tmp_path / (digest(raw)[7:] + ".json")
    path.write_bytes(raw)
    assert load_image_upload(path) == value
    path.write_bytes(raw + b"\n")
    with pytest.raises(ExportError, match="Invalid image upload description"):
        load_image_upload(path)
    path.write_bytes(raw.replace(b'"revision":7', b'"revision":6'))
    with pytest.raises(ExportError, match="Invalid image upload description"):
        load_image_upload(path)


@pytest.mark.parametrize(
    "changes",
    [
        {"key": "images/card_s/123.webp?v=7"},
        {"key": "../private.webp"},
        {"version": True},
        {"version": 0},
        {"bytes": -1},
        {"sha256": "not-a-hash"},
        {"source": "private-source"},
    ],
)
def test_inventory_rejects_invalid_or_private_rows(
    changes: dict[str, JsonValue],
) -> None:
    row = item().model_dump(mode="json") | changes
    with pytest.raises(ExportError, match="Invalid image upload inventory"):
        read_image_inventory(
            canonical({"format": "sve-publish-images-v1", "objects": [row]})
        )


@pytest.mark.parametrize("case", ["duplicate", "unsorted", "format", "noncanonical"])
def test_inventory_requires_unique_ordered_canonical_rows(case: str) -> None:
    first = item()
    second = first.model_copy(update={"key": "images/card_s/124.webp"})
    rows = (
        (first, first)
        if case == "duplicate"
        else (second, first)
        if case == "unsorted"
        else (first,)
    )
    raw = canonical(
        {
            "format": "unknown" if case == "format" else "sve-publish-images-v1",
            "objects": [row.model_dump(mode="json") for row in rows],
        }
    )
    if case == "noncanonical":
        raw += b"\n"
    with pytest.raises(ExportError, match="Invalid image upload inventory"):
        read_image_inventory(raw)
    assert (
        read_image_inventory(
            ImageInventory(format="sve-publish-images-v1", objects=()).content()
        ).objects
        == ()
    )
