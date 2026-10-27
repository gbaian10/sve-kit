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


def plan_images(
    description: ImageUpload,
    old: dict[str, ImageObject],
    selected: frozenset[str] | None = None,
) -> dict[str, object]:
    """Report all differences while limiting transfer candidates to the selected keys."""
    objects = tuple(
        item for item in description.objects if selected is None or item.key in selected
    )
    changed, transfers = _changes(objects, old)
    result: dict[str, object] = {
        "mode": "dry_run",
        "remote": "checked",
        "image_files": len(objects),
        "image_bytes": sum(item.bytes for item in objects),
        "changed_keys": [item.key for item in changed],
        "transfer_keys": [item.key for item in transfers],
    }
    if selected is not None:
        expected = {item.key: item for item in description.objects}
        result["outside_scope_differences"] = [
            {
                "key": key,
                "reason": "remote_only"
                if key not in expected
                else "missing"
                if key not in old
                else "metadata_changed",
            }
            for key in sorted(expected.keys() | old.keys())
            if key not in selected and expected.get(key) != old.get(key)
        ]
    return result


def upload_images(
    store: R2Store,
    description: ImageUpload,
    root: Path,
    *,
    jobs: int = 16,
    progress: Callable[[str], None] | None = None,
    selected: frozenset[str] | None = None,
) -> dict[str, object]:
    """Failure leaves missing claims; recovery must use this same description and root."""
    directory(root)
    if jobs < 1:
        raise ExportError("Upload jobs must be at least 1")
    remote = store.get(INVENTORY)
    old = {item.key: item for item in read_inventory(remote).objects}
    report = plan_images(description, old, selected)
    objects = tuple(
        item for item in description.objects if selected is None or item.key in selected
    )
    changed, transfers = _changes(objects, old)
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
    result: dict[str, object] = {
        "mode": "execute",
        "written_images": pool.written,
        "unchanged_images": len(objects) - len(changed),
        "version_only_images": len(changed) - len(transfers),
        "cdn_verification": "not_checked",
    }
    if selected is not None:
        result["outside_scope_differences"] = report["outside_scope_differences"]
    return result
