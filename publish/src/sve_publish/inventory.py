"""Trust successful image uploads recorded in one conditionally written control file."""

from typing import TYPE_CHECKING

from sve_carddb.export.read_api import ExportError, ImageInventory, read_image_inventory

from sve_publish.headers import INDEX_HEADERS

if TYPE_CHECKING:
    from collections.abc import Iterable

    from sve_carddb.export.read_api import ImageFile, ImageObject

    from sve_publish.adapter import R2Store, Stored

INVENTORY = "_publish/images.json"


def read_inventory(remote: Stored | None) -> ImageInventory:
    """Only an actual 404 is an uninitialized inventory."""
    if remote is None:
        return ImageInventory(format="sve-publish-images-v1", objects=())
    if remote.headers != INDEX_HEADERS:
        raise ExportError("Image inventory metadata differs from contract")
    return read_image_inventory(remote.raw)


def inventory(objects: Iterable[ImageObject]) -> ImageInventory:
    """Build the canonical key ordering for a control write."""
    return ImageInventory(
        format="sve-publish-images-v1",
        objects=tuple(sorted(objects, key=lambda item: item.key)),
    )


def commit_inventory(
    store: R2Store, before: Stored | None, value: ImageInventory
) -> Stored:
    """A lost write response is resolved by reading only this control object."""
    raw = value.content()
    try:
        success = store.put(
            INVENTORY,
            raw,
            INDEX_HEADERS,
            expected=None if before is None else before.etag,
        )
    except ExportError:
        result = store.get(INVENTORY)
        if result is None or result.raw != raw or result.headers != INDEX_HEADERS:
            raise ExportError(
                "Image inventory write is uncertain; rerun the same image input"
            ) from None
        return result
    if not success:
        raise ExportError("Image inventory changed concurrently; stop other writers")
    result = store.get(INVENTORY)
    if result is None or result.raw != raw or result.headers != INDEX_HEADERS:
        raise ExportError(
            "Image inventory read-back differs; rerun the same image input"
        )
    return result


def require_images(remote: Stored | None, images: Iterable[ImageFile]) -> None:
    """A snapshot requires exact image versions and byte lengths, never a lower bound."""
    rows = {item.key: item for item in read_inventory(remote).objects}
    for image in images:
        item = rows.get(image.key)
        version = int(image.url.split("?v=")[1])
        if item is None or item.version < version:
            raise ExportError("Images are not ready; run images upload first")
        if item.version > version:
            raise ExportError(
                "Snapshot pins older images; rebuild with the current image index"
            )
        if item.bytes != image.bytes:
            raise ExportError("Image inventory bytes differ from the snapshot")
