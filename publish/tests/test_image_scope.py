"""Scoped handoffs report all bucket differences without touching excluded objects."""

import json
from typing import TYPE_CHECKING

import pytest
from sve_carddb.export.image_scope import ScopeBinding
from sve_carddb.export.read_api import ImageObject, ImageScope, ImageUpload, digest
from typer.testing import CliRunner

from sve_publish.adapter import Stored
from sve_publish.cli import app
from sve_publish.headers import INDEX_HEADERS
from sve_publish.images import plan_images, upload_images
from sve_publish.inventory import INVENTORY, inventory, read_inventory

from .r2_fixtures import ACCOUNT, BUCKET
from .r2_fixtures import remote as remote  # ruff: ignore[useless-import-alias] -- isolated SDK backend
from .r2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- backend dependency
from .r2_sdk_fixtures import install_mock_sdk

if TYPE_CHECKING:
    from pathlib import Path

    from sve_publish.adapter import R2Store

    from .r2_fixtures import Loopback, ServerState

pytestmark = pytest.mark.usefixtures("close_sdk_clients")


def handoff(root: Path) -> tuple[ImageUpload, Path, Path]:
    objects = []
    for number in (1, 2):
        key = f"images/card_s/{number}.webp"
        raw = f"new image {number}".encode()
        target = root / key
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(raw)
        objects.append(
            ImageObject(key=key, version=2, sha256=digest(raw), bytes=len(raw))
        )
    description = ImageUpload(
        format="sve-image-upload-v1",
        image_index_sha256="sha256:" + "a" * 64,
        revision=2,
        objects=tuple(objects),
    )
    path = root / (digest(description.content())[7:] + ".json")
    path.write_bytes(description.content())
    scope = ImageScope(
        image_upload_sha256=digest(description.content()),
        bindings=(
            ScopeBinding(printing_id="p:1", home_set_id="BP22", keys=(objects[0].key,)),
            ScopeBinding(printing_id="p:2", home_set_id="PR", keys=(objects[1].key,)),
        ),
    )
    manifest = root / (digest(scope.content())[7:] + ".json")
    manifest.write_bytes(scope.content())
    return description, path, manifest


def test_scoped_upload_preserves_different_excluded_claims_and_bytes(
    tmp_path: Path,
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    description, _, _ = handoff(tmp_path)
    excluded = description.objects[1].model_copy(
        update={"version": 3, "sha256": digest(b"old"), "bytes": 3}
    )
    state.objects[INVENTORY] = Stored(
        inventory((excluded,)).content(), '"old"', INDEX_HEADERS
    )
    state.objects[excluded.key] = Stored(b"old", '"old"', {})
    (tmp_path / excluded.key).unlink()
    result = upload_images(
        store, description, tmp_path, selected=frozenset({description.objects[0].key})
    )
    assert result["written_images"] == 1
    assert result["outside_scope_differences"] == [
        {"key": excluded.key, "reason": "metadata_changed"}
    ]
    assert state.objects[excluded.key].raw == b"old"
    assert excluded in read_inventory(state.objects[INVENTORY]).objects
    assert not any(
        op == "PUT" and key == excluded.key for op, key, _ in state.operations
    )


def test_scoped_dry_run_lists_only_selected_keys_and_all_excluded_differences(
    tmp_path: Path,
) -> None:
    description, _, _ = handoff(tmp_path)
    extra = description.objects[1].model_copy(update={"key": "images/card_s/9.webp"})
    result = plan_images(
        description, {extra.key: extra}, frozenset({description.objects[0].key})
    )
    assert result["transfer_keys"] == [description.objects[0].key]
    assert result["outside_scope_differences"] == [
        {"key": description.objects[1].key, "reason": "missing"},
        {"key": extra.key, "reason": "remote_only"},
    ]


def test_cli_offline_scope_and_printing_union_needs_no_bitmap_root(
    tmp_path: Path,
) -> None:
    description, path, manifest = handoff(tmp_path)
    remote_path = tmp_path / "remote.json"
    remote_path.write_bytes(inventory(()).content())
    result = CliRunner().invoke(
        app,
        [
            "images",
            "upload",
            "--image-upload",
            str(path),
            "--image-dir",
            str(tmp_path / "absent"),
            "--scope-manifest",
            str(manifest),
            "--scope",
            "BP22",
            "--printing-id",
            "p:2",
            "--remote-inventory",
            str(remote_path),
        ],
    )
    assert result.exit_code == 0, result.output
    assert json.loads(result.stdout)["transfer_keys"] == [
        item.key for item in description.objects
    ]
    assert json.loads(result.stdout)["outside_scope_differences"] == []


def test_scoped_live_dry_run_only_reads_inventory(
    tmp_path: Path,
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _, state, transport = remote
    description, path, manifest = handoff(tmp_path)
    state.objects[INVENTORY] = Stored(inventory(()).content(), '"empty"', INDEX_HEADERS)
    install_mock_sdk(monkeypatch, transport)
    monkeypatch.setenv("SVE_R2_ACCESS_KEY_ID", "synthetic-access")
    monkeypatch.setenv("SVE_R2_SECRET_ACCESS_KEY", "synthetic-secret")
    result = CliRunner().invoke(
        app,
        [
            "images",
            "upload",
            "--image-upload",
            str(path),
            "--image-dir",
            str(tmp_path / "absent"),
            "--scope-manifest",
            str(manifest),
            "--scope",
            "BP22",
            "--account-id",
            ACCOUNT,
            "--bucket",
            BUCKET,
        ],
    )
    assert result.exit_code == 0, result.output
    assert json.loads(result.stdout)["transfer_keys"] == [description.objects[0].key]
    assert [
        (method, path.rsplit("/", 2)[-2:]) for method, path, _ in state.requests
    ] == [("GET", ["_publish", "images.json"])]
    assert not state.operations


def test_cli_rejects_unknown_printing_before_network(tmp_path: Path) -> None:
    _, path, manifest = handoff(tmp_path)
    result = CliRunner().invoke(
        app,
        [
            "images",
            "upload",
            "--image-upload",
            str(path),
            "--image-dir",
            str(tmp_path),
            "--scope-manifest",
            str(manifest),
            "--printing-id",
            "p:unknown",
        ],
    )
    assert result.exit_code != 0
    assert "p:unknown" in result.output
