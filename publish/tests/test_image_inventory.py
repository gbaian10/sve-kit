"""Image claims, independent snapshot publication and interrupted fixed-key writes."""

import json
import shutil
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from sve_carddb.export.read_api import (
    INDEX,
    ExportError,
    load_export,
    load_image_upload,
)
from typer.testing import CliRunner

from sve_publish.adapter import IMAGE_PREFIXES, SNAPSHOT_PREFIXES, Stored
from sve_publish.cli import app
from sve_publish.gc import collect, collect_images
from sve_publish.headers import INDEX_HEADERS
from sve_publish.images import upload_images
from sve_publish.inventory import INVENTORY, inventory, read_inventory
from sve_publish.publish import upload

from .r2_export_fixtures import art_changed as art_changed  # ruff: ignore[useless-import-alias] -- synthetic changed crop
from .r2_export_fixtures import export, image_description
from .r2_export_fixtures import images as images  # ruff: ignore[useless-import-alias] -- synthetic corpus
from .r2_export_fixtures import roots as roots  # ruff: ignore[useless-import-alias] -- isolated outputs
from .r2_fixtures import ACCOUNT, BUCKET
from .r2_fixtures import remote as remote  # ruff: ignore[useless-import-alias] -- fake SDK backend
from .r2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- backend dependency
from .r2_sdk_fixtures import install_mock_sdk

if TYPE_CHECKING:
    from sve_carddb.export.preview import Roots

    from sve_publish.adapter import R2Store

    from .r2_fixtures import Loopback, ServerState
    from .synthetic_images import PublicImages

pytestmark = pytest.mark.usefixtures("close_sdk_clients")


def no_bitmaps(monkeypatch: pytest.MonkeyPatch) -> None:
    original = Path.read_bytes

    def read(path: Path) -> bytes:
        if path.suffix == ".webp":
            raise AssertionError("unchanged image was read")
        return original(path)

    monkeypatch.setattr(Path, "read_bytes", read)


def test_baseline_reuploads_existing_objects_then_unchanged_reads_no_images(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    description = image_description(roots.preview)
    state.objects[description.objects[0].key] = Stored(b"old", '"old"', {})
    first = upload_images(store, description, roots.preview)
    assert first["written_images"] == len(description.objects)
    assert not any(
        op == "GET" and key.startswith("images/") for op, key, _ in state.operations
    )
    assert read_inventory(state.objects[INVENTORY]).objects == description.objects
    before = len(state.operations)
    no_bitmaps(monkeypatch)
    result = upload_images(store, description, roots.preview)
    assert result["written_images"] == 0
    assert not any(op == "PUT" for op, _, _ in state.operations[before:])
    assert not any(
        op == "GET" and key.startswith("images/")
        for op, key, _ in state.operations[before:]
    )


def test_snapshot_needs_no_bitmap_root_and_reads_no_remote_image(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    store, state, transport = remote
    export(images, roots, step=0)
    upload_images(store, image_description(roots.preview), roots.preview)
    shutil.rmtree(roots.preview / "images")
    no_bitmaps(monkeypatch)
    loaded = load_export(roots.preview, verify_images=False)
    start = len(state.operations)
    assert upload(store, loaded)["index"] == "updated"
    assert not any(key.startswith("images/") for _, key, _ in state.operations[start:])
    install_mock_sdk(monkeypatch, transport)
    monkeypatch.setenv("SVE_R2_ACCESS_KEY_ID", "synthetic-access")
    monkeypatch.setenv("SVE_R2_SECRET_ACCESS_KEY", "synthetic-secret")
    result = CliRunner().invoke(
        app,
        [
            "snapshot",
            "upload",
            "--export-dir",
            str(roots.preview),
            "--execute",
            "--account-id",
            ACCOUNT,
            "--bucket",
            BUCKET,
        ],
    )
    assert result.exit_code == 0, result.output
    assert json.loads(result.stdout)["cdn_verification"] == "not_checked"
    assert "synthetic-secret" not in result.output


@pytest.mark.parametrize("case", ["missing", "older", "newer", "bytes"])
def test_snapshot_refuses_missing_or_inexact_image_claims(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    case: str,
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    description = image_description(roots.preview)
    item = description.objects[0]
    if case == "older":
        description = description.model_copy(
            update={
                "objects": tuple(
                    row.model_copy(update={"version": 2}) for row in description.objects
                )
            }
        )
        upload_images(store, description, roots.preview)
        loaded = load_export(roots.preview, verify_images=False)
        from dataclasses import replace  # ruff: ignore[import-outside-top-level] -- use an otherwise verified snapshot with a later image requirement

        loaded = replace(
            loaded, images=tuple(replace(i, url=i.key + "?v=3") for i in loaded.images)
        )
    else:
        if case == "newer":
            item = item.model_copy(update={"version": 2})
        elif case == "bytes":
            item = item.model_copy(update={"bytes": item.bytes + 1})
        rows = () if case == "missing" else (item, *description.objects[1:])
        state.objects[INVENTORY] = Stored(
            inventory(rows).content(), '"test"', INDEX_HEADERS
        )
        loaded = load_export(roots.preview, verify_images=False)
    with pytest.raises(
        ExportError, match=r"Images are not ready|older images|bytes differ"
    ):
        upload(store, loaded)
    assert INDEX not in state.objects


@pytest.mark.parametrize("fault", ["image", "invalidate", "commit"])
def test_failed_overwrite_leaves_no_false_ready_claim_and_same_input_recovers(
    images: PublicImages,
    art_changed: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    fault: str,
) -> None:
    store, state, transport = remote
    export(images, roots, step=0)
    upload_images(store, image_description(roots.preview), roots.preview)
    export(art_changed, roots, step=1)
    description = image_description(roots.preview)
    old_claims = state.objects[INVENTORY]
    if fault in {"invalidate", "commit"}:
        transport.reject_put_key = INVENTORY
        transport.reject_put_number = 1 if fault == "invalidate" else 2
    if fault == "image":
        state.fail_key = next(
            item.key
            for item in description.objects
            if item.key.startswith("images/art_")
        )
    start = len(state.operations)
    with pytest.raises(ExportError, match=r"concurrently|rerun the same"):
        upload_images(store, description, roots.preview, jobs=1)
    if fault == "invalidate":
        assert state.objects[INVENTORY] == old_claims
        assert not any(
            op == "PUT" and key.startswith("images/")
            for op, key, _ in state.operations[start:]
        )
    else:
        assert all(
            not row.key.startswith("images/art_")
            for row in read_inventory(state.objects[INVENTORY]).objects
        )
        with pytest.raises(ExportError, match="not ready"):
            upload(store, load_export(roots.preview, verify_images=False))
    state.fail_key = None
    upload_images(store, description, roots.preview)
    assert read_inventory(state.objects[INVENTORY]).objects == description.objects
    assert (
        upload(store, load_export(roots.preview, verify_images=False))["index"]
        == "updated"
    )


def test_version_only_change_writes_no_image_and_preserves_unrelated_rows(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    original = image_description(roots.preview)
    upload_images(store, original, roots.preview)
    changed = original.model_copy(
        update={
            "objects": (original.objects[0].model_copy(update={"version": 2}),),
            "revision": 2,
        }
    )
    no_bitmaps(monkeypatch)
    start = len(state.operations)
    result = upload_images(store, changed, roots.preview)
    assert result["version_only_images"] == 1
    assert not any(
        op == "PUT" and key.startswith("images/")
        for op, key, _ in state.operations[start:]
    )
    rows = {item.key: item for item in read_inventory(state.objects[INVENTORY]).objects}
    assert rows[changed.objects[0].key].version == 2
    assert len(rows) == len(original.objects)
    with pytest.raises(ExportError, match="backwards"):
        upload_images(store, original, roots.preview)
    bad = changed.model_copy(
        update={
            "objects": (
                changed.objects[0].model_copy(update={"sha256": "sha256:" + "b" * 64}),
            )
        }
    )
    with pytest.raises(ExportError, match="without a new event version"):
        upload_images(store, bad, roots.preview)


def test_bad_local_candidate_never_enters_inventory(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    description = image_description(roots.preview)
    (roots.preview / description.objects[0].key).write_bytes(b"damaged")
    with pytest.raises(ExportError, match="bytes differ"):
        upload_images(store, description, roots.preview, jobs=1)
    assert INVENTORY not in state.objects


def test_lost_control_response_is_resolved_without_reading_images(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, transport = remote
    export(images, roots, step=0)
    transport.lose_key = INVENTORY
    upload_images(store, image_description(roots.preview), roots.preview)
    assert (
        read_inventory(state.objects[INVENTORY]).objects
        == image_description(roots.preview).objects
    )


@pytest.mark.parametrize("case", ["json", "format", "fields", "headers", "forbidden"])
def test_invalid_inventory_never_becomes_an_empty_baseline(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    case: str,
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    raw = inventory(image_description(roots.preview).objects).content()
    if case == "json":
        raw = b"broken"
    elif case == "format":
        raw = raw.replace(b"sve-publish-images-v1", b"unknown")
    elif case == "fields":
        raw = raw.replace(b'"format":', b'"secret":"forbidden","format":')
    state.objects[INVENTORY] = Stored(
        raw, '"test"', {} if case == "headers" else INDEX_HEADERS
    )
    if case == "forbidden":
        state.status = 403
    with pytest.raises(ExportError, match=r"inventory|read failed"):
        upload_images(store, image_description(roots.preview), roots.preview)
    assert not any(op == "PUT" for op, _, _ in state.operations)


def test_image_gc_removes_claim_before_failed_delete_and_finds_orphans_on_rerun(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    description = image_description(roots.preview)
    upload_images(store, description, roots.preview)
    upload(store, load_export(roots.preview, verify_images=False))
    stray = description.objects[0].model_copy(update={"key": "images/card_s/999.webp"})
    state.objects[stray.key] = Stored(b"stray", '"stray"', {})
    state.objects[INVENTORY] = Stored(
        inventory((*description.objects, stray)).content(), '"extra"', INDEX_HEADERS
    )
    state.delete_status = 500
    with pytest.raises(ExportError, match="DELETE failed"):
        collect_images(store, IMAGE_PREFIXES, execute=True)
    assert stray.key in state.objects
    assert stray not in read_inventory(state.objects[INVENTORY]).objects
    state.delete_status = None
    assert collect_images(store, IMAGE_PREFIXES, execute=True)["deleted"] == [stray.key]
    assert state.page_size < len(description.objects)


def test_snapshot_gc_never_reads_or_lists_images_or_inventory(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    upload_images(store, image_description(roots.preview), roots.preview)
    upload(store, load_export(roots.preview, verify_images=False))
    state.objects[INVENTORY] = Stored(b"damaged", '"bad"', {})
    for item in image_description(roots.preview).objects:
        del state.objects[item.key]
    start = len(state.requests)
    assert collect(store, SNAPSHOT_PREFIXES, execute=True)["deleted"] == []
    assert all("images" not in path for _, path, _ in state.requests[start:])


def test_image_cli_dry_run_is_metadata_only_and_execute_uses_fake_sdk(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _, state, transport = remote
    export(images, roots, step=0)
    path = next((roots.preview / "image-upload").glob("*.json"))
    arguments = [
        "images",
        "upload",
        "--image-upload",
        str(path),
        "--image-dir",
        str(roots.preview),
    ]
    with monkeypatch.context() as patch:
        no_bitmaps(patch)
        dry = CliRunner().invoke(app, arguments)
        assert dry.exit_code == 0, dry.output
        assert json.loads(dry.stdout)["remote"] == "not_checked"
        assert not state.requests
    install_mock_sdk(monkeypatch, transport)
    monkeypatch.setenv("SVE_R2_ACCESS_KEY_ID", "synthetic-access")
    monkeypatch.setenv("SVE_R2_SECRET_ACCESS_KEY", "synthetic-secret")
    result = CliRunner().invoke(
        app, [*arguments, "--execute", "--account-id", ACCOUNT, "--bucket", BUCKET]
    )
    assert result.exit_code == 0, result.output
    assert json.loads(result.stdout)["written_images"] > 0
    assert load_image_upload(path) == image_description(roots.preview)


def test_unconditional_put_cannot_write_json(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, _, _ = remote
    with pytest.raises(ExportError, match="public WebP key"):
        store.put_image(INDEX, b"forbidden")


@pytest.mark.parametrize("case", ["missing-image", "missing-claim", "invalidation"])
def test_image_gc_refuses_incomplete_retention_and_failed_invalidation(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    case: str,
) -> None:
    store, state, transport = remote
    export(images, roots, step=0)
    description = image_description(roots.preview)
    upload_images(store, description, roots.preview)
    upload(store, load_export(roots.preview, verify_images=False))
    first = description.objects[0]
    stray = first.model_copy(update={"key": "images/card_s/999.webp"})
    state.objects[stray.key] = Stored(b"stray", '"stray"', {})
    if case == "missing-image":
        del state.objects[first.key]
    elif case == "missing-claim":
        state.objects[INVENTORY] = Stored(
            inventory(description.objects[1:]).content(), '"missing"', INDEX_HEADERS
        )
    else:
        state.objects[INVENTORY] = Stored(
            inventory((*description.objects, stray)).content(), '"extra"', INDEX_HEADERS
        )
        transport.reject_put_key = INVENTORY
        transport.reject_put_number = 1
    start = len(state.operations)
    with pytest.raises(ExportError, match=r"incomplete|concurrently"):
        collect_images(store, IMAGE_PREFIXES, execute=True)
    assert not any(op == "DELETE" for op, _, _ in state.operations[start:])
    assert stray.key in state.objects


def test_control_server_failure_does_not_write_an_image_baseline(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    description = image_description(roots.preview)
    upload_images(store, description, roots.preview)
    newer = description.model_copy(
        update={
            "objects": tuple(
                row.model_copy(update={"version": 2}) for row in description.objects
            )
        }
    )
    state.fail_key = INVENTORY
    start = len(state.operations)
    with pytest.raises(ExportError, match="write is uncertain"):
        upload_images(store, newer, roots.preview)
    assert not any(
        op == "PUT" and key.startswith("images/")
        for op, key, _ in state.operations[start:]
    )
    assert read_inventory(state.objects[INVENTORY]).objects == description.objects
