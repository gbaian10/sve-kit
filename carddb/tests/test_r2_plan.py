"""Public closure, create-only S3 writes and atomic pointer failure counterexamples."""

import gzip
import json
import os
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- only monkeypatched calls, never a real subprocess
from typing import TYPE_CHECKING

import httpx
import pytest
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.r2_upload.compression import command_brotli
from sve_carddb.r2_upload.plan import POINTER, UploadError, plan_preview, read_member
from sve_carddb.r2_upload.s3 import Credentials
from sve_carddb.r2_upload.upload import upload
from sve_carddb.snapshot.export import Brotli, export_snapshot
from sve_carddb.snapshot.preview import Roots, write_preview
from sve_carddb.snapshot.values import canonical, digest, object_value, parse

from .r2_upload_fixtures import Store, synthetic_transport
from .r2_upload_fixtures import local as local  # ruff: ignore[useless-import-alias] -- register shared test fixtures
from .r2_upload_fixtures import previous as previous  # ruff: ignore[useless-import-alias] -- genuinely complete old published version
from .r2_upload_fixtures import public_plan as public_plan  # ruff: ignore[useless-import-alias] -- register the validated immutable module base
from .r2_upload_fixtures import public_template as public_template  # ruff: ignore[useless-import-alias] -- register shared test fixtures
from .test_image_variants import png
from .test_snapshot_export import BATCH
from .test_snapshot_preview_images import images as images  # ruff: ignore[useless-import-alias] -- register the synthetic image template

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.r2_upload.plan import Plan

    from .test_snapshot_preview_images import PublicImages


def altered_manifest(local: Plan, extra: dict[str, str]) -> None:
    old = next(m.key for m in local.members if m.phase == 2 and m.key.endswith(".json"))
    value = object_value(parse(read_member(local.root, old))) | extra
    raw = canonical(value)
    path = "snapshots/manifests/" + digest(raw)[7:] + ".json"
    (local.root / old).unlink()
    (local.root / (old + ".gz")).unlink()
    (local.root / path).write_bytes(raw)
    (local.root / (path + ".gz")).write_bytes(gzip.compress(raw, mtime=0))
    (local.root / POINTER).write_bytes(
        canonical({"manifest_path": path, "manifest_sha256": digest(raw)})
    )


@pytest.mark.parametrize("flags", [[], ["--dry-run"]])
def test_default_cli_is_offline_and_reconciles_all_public_members(
    local: Plan, monkeypatch: pytest.MonkeyPatch, flags: list[str]
) -> None:
    def forbidden(*_args: object, **_kwargs: object) -> None:
        pytest.fail(
            "Offline plan must not read credentials or construct an HTTP client"
        )

    monkeypatch.setattr(Credentials, "environment", forbidden)
    monkeypatch.setattr(httpx, "Client", forbidden)
    result = CliRunner().invoke(
        app, ["r2", "upload-preview", "--preview-dir", str(local.root), *flags]
    )
    assert result.exit_code == 0, result.output
    report = json.loads(result.output)
    assert report == local.report()
    assert report["candidate_files"] == len(local.members)
    assert report["candidate_bytes"] == sum(m.size for m in local.members)
    assert sum(report["files_by_kind"].values()) == len(local.members)
    assert sum(report["bytes_by_kind"].values()) == report["candidate_bytes"]
    assert report["remote_existence"] == "not_checked"
    assert str(local.root) not in result.output
    assert "never read or upload" not in result.output


@pytest.mark.parametrize(
    ("args", "message"),
    [
        (["--execute"], "Execution requires contemporary maintainer authorization"),
        (
            ["--execute", "--confirm-maintainer-authorization"],
            "Execution requires an explicit R2 account ID and bucket",
        ),
    ],
)
def test_cli_execution_requires_explicit_current_authorization_and_target(
    local: Plan, args: list[str], message: str
) -> None:
    result = CliRunner().invoke(
        app, ["r2", "upload-preview", "--preview-dir", str(local.root), *args]
    )
    assert result.exit_code != 0
    assert message in result.output


@pytest.mark.parametrize(
    ("key", "message"),
    [
        ("images/original.png", "Unsupported public member"),
        ("snapshots/report.json", "Unsupported public member"),
        (
            "snapshots/manifests/" + "f" * 64 + ".json.br.gz",
            "Unsupported public member",
        ),
        ("recipe.json", "Unexpected entry at preview root"),
    ],
)
def test_unknown_public_files_are_rejected(local: Plan, key: str, message: str) -> None:
    path = local.root / key
    path.parent.mkdir(exist_ok=True, parents=True)
    path.write_bytes(b"synthetic private material")
    with pytest.raises(UploadError, match="^" + message + "$"):
        plan_preview(local.root)


def test_unreferenced_content_addressed_private_recipe_cannot_be_uploaded(
    local: Plan,
) -> None:
    raw = canonical({"source_root": "/home/synthetic/private", "recipe": "private"})
    (local.root / ("snapshots/blobs/" + digest(raw)[7:] + ".json")).write_bytes(raw)
    with pytest.raises(
        UploadError, match=r"^Public tree contains unreferenced or private members$"
    ):
        plan_preview(local.root)


@pytest.mark.parametrize(
    "path",
    [
        "/home/synthetic/source",
        "/srv/synthetic/source",
        "file:///private/input",
        "C:\\private\\input",
    ],
)
def test_public_json_cannot_hide_local_paths(local: Plan, path: str) -> None:
    altered_manifest(local, {"private_recipe": path})
    with pytest.raises(UploadError, match=r"^Public content contains a local path$"):
        plan_preview(local.root)


def test_private_fields_are_rejected_even_without_local_paths(local: Plan) -> None:
    altered_manifest(local, {"private_recipe": "synthetic-only"})
    with pytest.raises(UploadError, match=r"^Public preview validation failed$"):
        plan_preview(local.root)


@pytest.mark.parametrize("target", ["root", "subtree", "parent", "member", "fifo"])
def test_symlinks_and_special_files_are_never_followed(
    local: Plan, tmp_path: Path, target: str
) -> None:
    if target == "root":
        link = tmp_path / "alias"
        link.symlink_to(local.root, target_is_directory=True)
        root, message = link, "Preview root must be an absolute non-symlink directory"
    elif target == "subtree":
        images_dir = local.root / "images"
        images_dir.rename(local.root / "private/image-input")
        images_dir.symlink_to(
            local.root / "private/image-input", target_is_directory=True
        )
        root, message = local.root, "Public subtree must be a non-symlink directory"
    elif target == "parent":
        (local.root / "images/alias").symlink_to(tmp_path, target_is_directory=True)
        root, message = local.root, "Public subtree contains a symlink directory"
    else:
        member = next(m for m in local.members if m.phase == 0)
        path = local.root / member.key
        path.unlink()
        if target == "member":
            path.symlink_to(local.root / "private/recipe.json")
        else:
            os.mkfifo(path)
        root, message = local.root, "Public member is not a regular non-symlink file"
    with pytest.raises(UploadError, match="^" + message + "$"):
        plan_preview(root)


@pytest.mark.parametrize(
    "damage", ["pointer", "manifest", "payload", "gzip", "image", "missing-image"]
)
def test_broken_public_closure_stops_before_any_network(
    local: Plan, damage: str
) -> None:
    key = next(
        m.key
        for m in local.members
        if m.phase
        == {
            "pointer": 3,
            "manifest": 2,
            "payload": 1,
            "gzip": 1,
            "image": 0,
            "missing-image": 0,
        }[damage]
        and (m.key.endswith(".gz") if damage == "gzip" else not m.key.endswith(".gz"))
    )
    path = local.root / key
    if damage == "missing-image":
        path.unlink()
    else:
        path.write_bytes(b"synthetic corruption")
    store = Store()
    with pytest.raises(UploadError):
        upload(local, store.remote())
    assert store.calls == []


def test_explicit_brotli_producer_is_required_and_verified(
    images: PublicImages, tmp_path: Path
) -> None:
    codec = Brotli("synthetic-protocol", lambda raw: b"synthetic-br:" + raw)
    snapshot = export_snapshot(images.projection, images.ownership, BATCH, brotli=codec)
    roots = Roots(tmp_path / "br", tmp_path / "formal")
    write_preview(snapshot, roots, {}, brotli=codec, image_source=images.library)
    with pytest.raises(
        UploadError, match=r"^Brotli members require the explicit producer compressor$"
    ):
        plan_preview(roots.preview)
    plan = plan_preview(roots.preview, brotli=codec)
    br = next(m for m in plan.members if m.key.endswith(".br"))
    (roots.preview / br.key).write_bytes(b"synthetic bad br")
    with pytest.raises(UploadError, match=r"^Inconsistent Brotli member$"):
        plan_preview(roots.preview, brotli=codec)


@pytest.mark.parametrize(
    "base", ["https://official.invalid/card/", "https://user@official.invalid/card/"]
)
def test_root_relative_image_href_requires_a_public_https_base(
    images: PublicImages, tmp_path: Path, base: str
) -> None:
    tables = images.tables()
    tables["image_asset"][0] |= {
        "source_src_raw": "/assets/synthetic.webp",
        "source_url": base,
    }
    root = synthetic_transport(images, tables, tmp_path / "preview")
    if "user@" not in base:
        assert sum(member.phase == 0 for member in plan_preview(root).members) == len(
            {row["path"] for row in images.tables()["image_variant"]}
        )
    else:
        with pytest.raises(
            UploadError,
            match=r"^Root-relative image source requires a public HTTPS base$",
        ):
            plan_preview(root)


@pytest.mark.parametrize("field", ["source_src_raw", "source_url"])
def test_image_payload_cannot_hide_a_local_path(
    images: PublicImages, tmp_path: Path, field: str
) -> None:
    tables = images.tables()
    tables["image_asset"][0][field] = (
        "/home/synthetic/private"
        if field == "source_src_raw"
        else "https://official.invalid/?redirect=/home/synthetic/private"
    )
    root = synthetic_transport(images, tables, tmp_path / "preview")
    with pytest.raises(UploadError, match=r"^Public content contains a local path$"):
        plan_preview(root)


@pytest.mark.parametrize("damage", ["png", "dimensions", "missing-size"])
def test_public_image_bytes_and_size_set_are_independently_verified(
    images: PublicImages, tmp_path: Path, damage: str
) -> None:
    tables = images.tables()
    variant = tables["image_variant"][0]
    replacements = {}
    if damage == "png":
        raw = png(8, 8)
        hashed = digest(raw)[7:]
        key = f"images/sha256/{hashed[:2]}/{hashed}.webp"
        variant |= {"path": key, "bytes": len(raw), "width": 8, "height": 8}
        replacements[key] = raw
    elif damage == "dimensions":
        for row in tables["image_variant"]:
            if row["path"] == variant["path"]:
                row["width"] = 1
    else:
        tables["image_variant"].pop()
    root = synthetic_transport(images, tables, tmp_path / "preview", replacements)
    message = (
        "Available public image requires all five sizes"
        if damage == "missing-size"
        else "Public WebP format or dimensions mismatch"
    )
    with pytest.raises(UploadError, match="^" + message + "$"):
        plan_preview(root)


def test_local_encoder_protocol_and_redaction(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    command = tmp_path / "encoder"
    command.write_bytes(b"synthetic local producer executable")

    def fake(
        argv: list[str],
        *,
        stderr: int,
        input: bytes | None = None,  # ruff: ignore[builtin-argument-shadowing] -- exact subprocess keyword protocol
    ) -> bytes:
        assert stderr == subprocess.DEVNULL
        assert argv[0] == str(command)
        if argv[1:] == ["--version"]:
            return b"synthetic producer-v1"
        assert argv[1:] == ["-q", "11", "-c"]
        assert input == b"synthetic input"
        return b"synthetic encoded"

    monkeypatch.setattr(subprocess, "check_output", fake)
    codec = command_brotli(command)
    assert codec.compress(b"synthetic input") == b"synthetic encoded"

    def failed(*_args: object, **_kwargs: object) -> bytes:
        raise subprocess.CalledProcessError(1, "synthetic private executable")

    monkeypatch.setattr(subprocess, "check_output", failed)
    with pytest.raises(UploadError, match=r"^Explicit Brotli compressor failed$"):
        codec.compress(b"synthetic input")
    with pytest.raises(UploadError, match=r"^Explicit Brotli compressor failed$"):
        command_brotli(command)
