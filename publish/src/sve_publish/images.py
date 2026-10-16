"""Upload only changed images; invalidate old claims before overwriting fixed keys."""

from functools import partial
from typing import TYPE_CHECKING

from sve_carddb.export.read_api import ExportError, digest, directory, read_member

from sve_publish.headers import IMAGE_HEADERS
from sve_publish.inventory import INVENTORY, commit_inventory, inventory, read_inventory
from sve_publish.parallel import UploadObject, UploadPool

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from sve_carddb.export.read_api import ImageObject, ImageUpload

    from sve_publish.adapter import R2Store


def image_bytes(root: Path, item: ImageObject) -> bytes:
    """Read and hash only candidates selected for PUT."""
    raw = read_member(root, item.key)
    if digest(raw) != item.sha256 or len(raw) != item.bytes:
        raise ExportError("Image bytes differ from the upload description")
    return raw


def _changes(
    objects: tuple[ImageObject, ...], old: dict[str, ImageObject]
) -> tuple[list[ImageObject], list[ImageObject]]:
    changed = []
    transfers = []
    for item in objects:
        before = old.get(item.key)
        if before == item:
            continue
        if before is not None:
            if item.version < before.version:
                raise ExportError(
                    "Image version moved backwards; restore the current producer input"
                )
            if item.version == before.version:
                raise ExportError("Image bytes changed without a new event version")
        changed.append(item)
        if before is None or (item.sha256, item.bytes) != (before.sha256, before.bytes):
            transfers.append(item)
    return changed, transfers


def upload_images(
    store: R2Store,
    description: ImageUpload,
    root: Path,
    *,
    jobs: int = 16,
    progress: Callable[[str], None] | None = None,
) -> dict[str, object]:
    """Failure leaves missing claims; recovery must use this same description and root."""
    directory(root)
    if jobs < 1:
        raise ExportError("Upload jobs must be at least 1")
    remote = store.get(INVENTORY)
    old = {item.key: item for item in read_inventory(remote).objects}
    changed, transfers = _changes(description.objects, old)
    affected = {item.key for item in changed}
    remaining = {key: item for key, item in old.items() if key not in affected}
    if old.keys() & affected:
        remote = commit_inventory(store, remote, inventory(remaining.values()))
    pool = UploadPool(store, jobs, len(transfers), progress)
    try:
        pool.sync(
            UploadObject(
                item.key,
                partial(image_bytes, root, item),
                IMAGE_HEADERS,
                image=True,
            )
            for item in transfers
        )
        if changed or remote is None:
            commit_inventory(
                store,
                remote,
                inventory((remaining | {item.key: item for item in changed}).values()),
            )
    except ExportError as error:
        raise ExportError(
            f"{error}; rerun the same image description and unchanged image root"
        ) from None
    return {
        "mode": "execute",
        "written_images": pool.written,
        "unchanged_images": len(description.objects) - len(changed),
        "version_only_images": len(changed) - len(transfers),
        "cdn_verification": "not_checked",
    }
