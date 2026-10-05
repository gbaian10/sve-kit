"""Offline default, pinned formal inputs, checkpoint and localhost execution."""

import json
from dataclasses import dataclass, replace
from shutil import copytree
from typing import TYPE_CHECKING

import brotli
import httpx
import pytest
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.r2_upload.sdk import Credentials
from sve_carddb.r2_upload.v2 import commands
from sve_carddb.r2_upload.v2.bundle import (
    load_bundle,
    report,
    save_checkpoint,
    verify_checkpoint,
    write_bundle,
)
from sve_carddb.snapshot.export.compression import python_brotli
from sve_carddb.snapshot.publish import Ledger, PublishError, Release, publish
from sve_carddb.snapshot.publish.plan import INDEX
from sve_carddb.snapshot.values import canonical, object_value, parse, string

from .r2_sdk_fixtures import install_mock_sdk
from .r2_v2_fixtures import ACCOUNT, BUCKET
from .r2_v2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- register shared localhost pytest fixture
from .snapshot_publish_fixtures import FakeCDN, FakeS3, candidate, version
from .snapshot_publish_fixtures import images as images  # ruff: ignore[useless-import-alias] -- shared module-scoped image library

pytestmark = pytest.mark.usefixtures("close_sdk_clients")

if TYPE_CHECKING:
    from pathlib import Path

    from .r2_v2_fixtures import Loopback, ServerState
    from .test_snapshot_preview_images import PublicImages


@dataclass(repr=False)
class Frozen:
    root: Path
    ledger: Ledger
    release: Release
    checkpoint: Path

    def args(self) -> list[str]:
        return [
            "r2",
            "upload-v2",
            "--release-dir",
            str(self.root),
            "--ledger-dir",
            str(self.ledger.root),
            "--backup-dir",
            str(self.ledger.backup),
            "--checkpoint-file",
            str(self.checkpoint),
            "--cdn-base-url",
            "https://cdn.invalid/",
        ]


@pytest.fixture(scope="module")
def frozen_base(
    images: PublicImages, tmp_path_factory: pytest.TempPathFactory
) -> Frozen:
    base = tmp_path_factory.mktemp("r2-v2-baseline")
    ledger = Ledger(base / "ledger", base / "backup")
    ledger.initialize()
    release = candidate(ledger, images)
    root = base / "bundle"
    root.mkdir()
    write_bundle(root, release)
    checkpoint = base / "checkpoint.json"
    save_checkpoint(checkpoint, ledger)
    return Frozen(root, ledger, release, checkpoint)


@pytest.fixture
def frozen(frozen_base: Frozen, tmp_path: Path) -> Frozen:
    root = tmp_path / "bundle"
    copytree(frozen_base.root, root)
    copytree(frozen_base.ledger.root, tmp_path / "ledger")
    copytree(frozen_base.ledger.backup, tmp_path / "backup")
    ledger = Ledger(tmp_path / "ledger", tmp_path / "backup")
    checkpoint = tmp_path / "checkpoint.json"
    save_checkpoint(checkpoint, ledger)
    return Frozen(root, ledger, frozen_base.release, checkpoint)


@pytest.fixture(scope="module")
def brotli_base(
    images: PublicImages, tmp_path_factory: pytest.TempPathFactory
) -> Frozen:
    base = tmp_path_factory.mktemp("r2-v2-brotli")
    ledger = Ledger(base / "ledger", base / "backup")
    ledger.initialize()
    first = candidate(ledger, images, brotli=python_brotli())
    store = FakeS3()
    publish(ledger, store, first, FakeCDN(store))
    release = candidate(
        ledger, images, from_version=version(first), brotli=python_brotli()
    )
    path = string(release.entry["manifest_path"]) + ".br"
    manifest = canonical(release.snapshot.manifest)
    alternate = brotli.compress(manifest, quality=4)
    assert alternate != next(m.raw for m in release.members if m.key == path)
    release = replace(
        release,
        members=tuple(
            replace(m, raw=alternate) if m.key == path else m for m in release.members
        ),
    )
    root = base / "bundle"
    root.mkdir()
    write_bundle(root, release)
    checkpoint = base / "checkpoint.json"
    save_checkpoint(checkpoint, ledger)
    return Frozen(root, ledger, release, checkpoint)


def test_frozen_brotli_transport_preserves_alternate_encoding_without_recompression(
    brotli_base: Frozen,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    def forbidden(*_args: object, **_kwargs: object) -> bytes:
        pytest.fail("Frozen validation must not recompress Brotli")

    monkeypatch.setattr(brotli, "compress", forbidden)
    actual = load_bundle(
        brotli_base.root, brotli_base.ledger, cdn_root="https://cdn.invalid/"
    )
    assert actual.members == brotli_base.release.members
    assert actual.entry == brotli_base.release.entry
    assert actual.snapshot == brotli_base.release.snapshot
    assert (
        report(actual, brotli_base.ledger)["candidate_bytes"]
        == report(brotli_base.release, brotli_base.ledger)["candidate_bytes"]
    )


@pytest.mark.parametrize("attachment", ["manifest", "changes"])
@pytest.mark.parametrize("encoding", ["raw", "gzip", "br"])
def test_corrupted_frozen_brotli_attachments_fail_before_credentials(
    brotli_base: Frozen,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    attachment: str,
    encoding: str,
) -> None:
    root = tmp_path / "bundle"
    copytree(brotli_base.root, root)
    key = (
        string(brotli_base.release.entry["manifest_path"])
        if attachment == "manifest"
        else string(
            object_value(brotli_base.release.snapshot.manifest["changes_ref"])["path"]
        )
    )
    suffix = {"raw": "", "gzip": ".gz", "br": ".br"}[encoding]
    path = root / (key + suffix)
    raw = path.read_bytes()
    path.write_bytes(bytes([raw[0] ^ 0xFF]) + raw[1:])

    def forbidden() -> Credentials:
        pytest.fail("Invalid transport must fail before credentials")

    monkeypatch.setattr(Credentials, "environment", forbidden)
    args = Frozen(
        root, brotli_base.ledger, brotli_base.release, brotli_base.checkpoint
    ).args()
    result = CliRunner().invoke(
        app, [*args, "--execute", "--confirm-maintainer-authorization"]
    )
    assert result.exit_code == 2


def test_brotli_cli_dry_run_never_reads_credentials(
    brotli_base: Frozen, monkeypatch: pytest.MonkeyPatch
) -> None:
    def forbidden() -> Credentials:
        pytest.fail("Offline Brotli validation must not read credentials")

    monkeypatch.setattr(Credentials, "environment", forbidden)
    result = CliRunner().invoke(app, [*brotli_base.args(), "--dry-run"])
    assert result.exit_code == 0, result.exception
    assert (
        json.loads(result.output)["candidate_files"]
        == len(brotli_base.release.members) + len(brotli_base.release.assets) + 1
    )


def test_roundtrip_preserves_exact_transport_assets_and_reservation(
    frozen: Frozen,
) -> None:
    actual = load_bundle(frozen.root, frozen.ledger, cdn_root="https://cdn.invalid/")
    assert actual.members == frozen.release.members
    assert actual.assets == frozen.release.assets
    assert actual.media.state == frozen.release.media.state
    assert actual.entry == frozen.release.entry
    assert dict(actual.images()) == dict(frozen.release.images())
    verify_checkpoint(frozen.checkpoint, frozen.ledger)
    totals = report(actual, frozen.ledger)
    assert totals["new_v_range"] == [1, 1]
    index_size = totals["index_bytes"]
    assert isinstance(index_size, int)
    assert totals["candidate_files"] == len(actual.assets) + len(actual.members) + 1
    assert (
        totals["candidate_bytes"]
        == sum(len(m.raw) for m in actual.members)
        + sum(len(raw) for _, raw in actual.images())
        + index_size
    )
    assert totals["would_collect"] == []


def test_default_dry_run_reads_no_credentials_creates_no_client_or_mutations(
    frozen: Frozen, monkeypatch: pytest.MonkeyPatch
) -> None:
    def forbidden(*_args: object, **_kwargs: object) -> None:
        pytest.fail("dry run touched credentials or network client")

    monkeypatch.setattr(Credentials, "environment", forbidden)
    monkeypatch.setattr(httpx, "Client", forbidden)
    monkeypatch.setattr("sve_carddb.r2_upload.sdk.Session", forbidden)
    before = {
        p: p.read_bytes()
        for root in (
            frozen.root,
            frozen.ledger.root,
            frozen.ledger.backup,
            frozen.checkpoint.parent,
        )
        for p in root.rglob("*")
        if p.is_file()
    }
    result = CliRunner().invoke(app, frozen.args())
    assert result.exit_code == 0, result.output
    assert json.loads(result.output)["mode"] == "offline_dry_run"
    after = {
        p: p.read_bytes()
        for root in (
            frozen.root,
            frozen.ledger.root,
            frozen.ledger.backup,
            frozen.checkpoint.parent,
        )
        for p in root.rglob("*")
        if p.is_file()
    }
    assert after == before


def test_execute_needs_current_authorization_before_reading_inputs(
    frozen: Frozen, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(
        commands,
        "load_bundle",
        lambda *_args, **_kwargs: pytest.fail("unauthorized read"),
    )
    result = CliRunner().invoke(app, [*frozen.args(), "--execute"])
    assert result.exit_code == 2
    assert "Execution requires contemporary maintainer authorization" in result.output


def test_frozen_preview_cannot_be_promoted(frozen: Frozen) -> None:
    descriptor = object_value(parse((frozen.root / "release.json").read_bytes()))
    path = frozen.root / string(descriptor["manifest_path"])
    manifest = object_value(parse(path.read_bytes()))
    manifest["data_version"] = "preview-20261004T000000Z-0001"
    path.write_bytes(canonical(manifest))
    with pytest.raises(
        PublishError, match=r"^Frozen manifest content address mismatch$"
    ):
        load_bundle(frozen.root, frozen.ledger, cdn_root="https://cdn.invalid/")


@pytest.mark.parametrize(
    "attack",
    [
        "unreferenced",
        "symlink",
        "traversal",
        "missing",
        "reserved-version",
        "media-token",
        "source-change",
        "root-member",
    ],
)
def test_invalid_frozen_input_is_rejected_before_credentials(
    frozen: Frozen, monkeypatch: pytest.MonkeyPatch, attack: str
) -> None:
    def forbidden(*_args: object, **_kwargs: object) -> None:
        pytest.fail("invalid bundle reached credentials or network")

    monkeypatch.setattr(Credentials, "environment", forbidden)
    descriptor = object_value(parse((frozen.root / "release.json").read_bytes()))
    if attack == "unreferenced":
        (frozen.root / "snapshots" / "unexpected.json").write_bytes(b"{}")
    elif attack == "root-member":
        (frozen.root / "unexpected.txt").write_bytes(b"synthetic root member")
    elif attack == "symlink":
        p = frozen.root / string(descriptor["manifest_path"])
        target = p.with_suffix(".original")
        p.rename(target)
        p.symlink_to(target)
    elif attack == "traversal":
        descriptor["manifest_path"] = "../secret.env"
    elif attack == "missing":
        (frozen.root / string(descriptor["manifest_path"])).unlink()
    elif attack == "reserved-version":
        object_value(descriptor["media_state"])["revision"] = 2
    elif attack == "media-token":
        object_value(descriptor["media_state"])["members"] = {}
    elif attack == "source-change":
        asset = frozen.release.assets[0]
        (frozen.root / "sources" / string(asset["source"])).write_bytes(
            b"changed image"
        )
    (frozen.root / "release.json").write_bytes(canonical(descriptor))
    result = CliRunner().invoke(
        app,
        [
            *frozen.args(),
            "--execute",
            "--confirm-maintainer-authorization",
            "--account-id",
            ACCOUNT,
            "--bucket",
            BUCKET,
        ],
    )
    assert result.exit_code == 2
    assert "synthetic-secret" not in result.output
    assert "secret.env" not in result.output


def test_missing_or_stale_independent_checkpoint_is_not_automatically_replaced(
    frozen: Frozen,
) -> None:
    frozen.checkpoint.write_bytes(
        canonical(
            {"high_water": 0, "state_sha256": "old", "reservations_sha256": "old"}
        )
    )
    before = frozen.checkpoint.read_bytes()
    result = CliRunner().invoke(app, frozen.args())
    assert result.exit_code == 2
    assert "Independent checkpoint differs from the verified ledger" in result.output
    assert frozen.checkpoint.read_bytes() == before
    frozen.checkpoint.unlink()
    result = CliRunner().invoke(app, frozen.args())
    assert result.exit_code == 2
    assert not frozen.checkpoint.exists()


def test_checkpoint_cannot_live_beside_primary_or_backup(frozen: Frozen) -> None:
    with pytest.raises(
        PublishError, match=r"^Checkpoint must be outside both ledger roots$"
    ):
        save_checkpoint(frozen.ledger.backup / "checkpoint.json", frozen.ledger)


def test_checkpoint_with_group_or_other_permissions_is_rejected(frozen: Frozen) -> None:
    frozen.checkpoint.chmod(0o677)
    with pytest.raises(
        PublishError, match=r"^Checkpoint requires a private regular file$"
    ):
        verify_checkpoint(frozen.checkpoint, frozen.ledger)


def test_ledger_not_automatically_created_in_dry_run(
    frozen: Frozen, tmp_path: Path
) -> None:
    args = frozen.args()
    args[args.index("--ledger-dir") + 1] = str(tmp_path / "nonexistent-ledger")
    result = CliRunner().invoke(app, args)
    assert result.exit_code == 2
    assert not (tmp_path / "nonexistent-ledger").exists()


def test_execute_and_identical_retry_use_v2_publisher_and_advance_checkpoint(
    frozen: Frozen,
    server: tuple[ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    state, transport = server
    real_client = httpx.Client
    configurations: list[dict[str, object]] = []

    def factory(**kwargs: object) -> httpx.Client:
        configurations.append(kwargs)
        return real_client(
            transport=transport, trust_env=False, follow_redirects=False, timeout=30
        )

    install_mock_sdk(monkeypatch, transport)
    monkeypatch.setattr(httpx, "Client", factory)
    monkeypatch.setenv("SVE_R2_ACCESS_KEY_ID", "synthetic-access")
    monkeypatch.setenv("SVE_R2_SECRET_ACCESS_KEY", "synthetic-secret")
    before = frozen.checkpoint.read_bytes()
    args = [
        *frozen.args(),
        "--execute",
        "--confirm-maintainer-authorization",
        "--account-id",
        ACCOUNT,
        "--bucket",
        BUCKET,
    ]
    result = CliRunner().invoke(app, args)
    assert result.exit_code == 0, result.output
    assert json.loads(result.output)["mode"] == "execute"
    assert state.objects[INDEX].headers["cache-control"] == "no-store"
    assert frozen.checkpoint.read_bytes() != before
    verify_checkpoint(frozen.checkpoint, frozen.ledger)
    assert all(
        not settings["trust_env"] and not settings["follow_redirects"]
        for settings in configurations
    )
    initial_index = state.objects[INDEX]
    writes_before = len(state.operations)
    retry = CliRunner().invoke(app, args)
    assert retry.exit_code == 0, retry.output
    assert state.objects[INDEX] == initial_index
    assert all(
        op[1].startswith("coordination/") for op in state.operations[writes_before:]
    )
    assert all(
        "synthetic-secret" not in r.output and "Signature=" not in r.output
        for r in (result, retry)
    )
    assert [op for op in state.operations if not op[1].startswith("coordination/")][-1][
        1
    ] == INDEX


def test_execute_can_skip_cdn_verification_but_reads_images_from_origin(
    frozen: Frozen,
    server: tuple[ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    state, transport = server
    real_client = httpx.Client

    def factory(**_kwargs: object) -> httpx.Client:
        return real_client(transport=transport, trust_env=False, follow_redirects=False)

    install_mock_sdk(monkeypatch, transport)
    monkeypatch.setattr(httpx, "Client", factory)
    monkeypatch.setenv("SVE_R2_ACCESS_KEY_ID", "synthetic-access")
    monkeypatch.setenv("SVE_R2_SECRET_ACCESS_KEY", "synthetic-secret")
    result = CliRunner().invoke(
        app,
        [
            *frozen.args(),
            "--execute",
            "--skip-cdn-verify",
            "--confirm-maintainer-authorization",
            "--account-id",
            ACCOUNT,
            "--bucket",
            BUCKET,
        ],
    )
    assert result.exit_code == 0, result.output
    output = json.loads(result.output)
    assert output["remote_existence"] == "verified"
    assert output["cdn_verification"] == "skipped"
    assert any(
        method == "GET" and path.startswith(f"/{BUCKET}/images/")
        for method, path, _headers in state.requests
    )
    assert not any(
        headers.get("host") == "cdn.invalid"
        for _method, _path, headers in state.requests
    )
    assert INDEX in state.objects


@pytest.mark.parametrize("cause", ["cdn-denied", "lost-image-response"])
def test_execute_failure_keeps_current_absent_records_checkpoint_and_resumes(
    frozen: Frozen,
    server: tuple[ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
    cause: str,
) -> None:
    state, transport = server
    real_client = httpx.Client

    def factory(**_kwargs: object) -> httpx.Client:
        return real_client(transport=transport, trust_env=False, follow_redirects=False)

    install_mock_sdk(monkeypatch, transport)
    monkeypatch.setattr(httpx, "Client", factory)
    monkeypatch.setenv("SVE_R2_ACCESS_KEY_ID", "synthetic-access")
    monkeypatch.setenv("SVE_R2_SECRET_ACCESS_KEY", "synthetic-secret")
    monkeypatch.setenv("R2_ACCOUNT_ID", ACCOUNT)
    monkeypatch.setenv("R2_DEV_BUCKET", BUCKET)
    if cause == "cdn-denied":
        state.cdn_status = 403
    else:
        transport.lose_key = string(frozen.release.assets[0]["path"])
    args = [*frozen.args(), "--execute", "--confirm-maintainer-authorization"]
    result = CliRunner().invoke(app, args)
    assert result.exit_code == 2
    if cause == "cdn-denied":
        assert any(
            headers.get("host") == "cdn.invalid"
            for _method, _path, headers in state.requests
        )
    assert INDEX not in state.objects
    assert "synthetic-secret" not in result.output
    assert "synthetic-access" not in result.output
    assert "Signature=" not in result.output
    verify_checkpoint(frozen.checkpoint, frozen.ledger)
    attempts = frozen.ledger.read()["attempts"]
    assert isinstance(attempts, list)
    assert object_value(attempts[0])["status"] == "failed"
    state.cdn_status = None
    retry = CliRunner().invoke(app, args)
    assert retry.exit_code == 0, retry.output
    verify_checkpoint(frozen.checkpoint, frozen.ledger)
    assert object_value(parse(state.objects[INDEX].raw))["revision"] == 1
    assert frozen.ledger.read()["high_water"] == 1


@pytest.mark.parametrize(
    "missing",
    [
        "SVE_R2_ACCESS_KEY_ID",
        "SVE_R2_SECRET_ACCESS_KEY",
        "R2_ACCOUNT_ID",
        "R2_DEV_BUCKET",
    ],
)
def test_missing_explicit_environment_never_opens_http(
    frozen: Frozen, monkeypatch: pytest.MonkeyPatch, missing: str
) -> None:
    def forbidden(**_kwargs: object) -> None:
        pytest.fail("missing credential or target reached HTTP")

    monkeypatch.setattr(httpx, "Client", forbidden)
    monkeypatch.setattr("sve_carddb.r2_upload.sdk.Session", forbidden)
    for key, value in {
        "SVE_R2_ACCESS_KEY_ID": "synthetic-access",
        "SVE_R2_SECRET_ACCESS_KEY": "synthetic-secret",
        "R2_ACCOUNT_ID": ACCOUNT,
        "R2_DEV_BUCKET": BUCKET,
    }.items():
        monkeypatch.setenv(key, value)
    monkeypatch.delenv(missing)
    result = CliRunner().invoke(
        app, [*frozen.args(), "--execute", "--confirm-maintainer-authorization"]
    )
    assert result.exit_code == 2
    assert "synthetic-access" not in result.output
    assert "synthetic-secret" not in result.output


@pytest.mark.parametrize("kind", ["version", "extra-field"])
def test_descriptor_rejects_unknown_format_or_fields(frozen: Frozen, kind: str) -> None:
    private = object_value(parse((frozen.root / "release.json").read_bytes()))
    if kind == "version":
        private["format"] = 2
    else:
        private["unapproved"] = True
    (frozen.root / "release.json").write_bytes(canonical(private))
    with pytest.raises(PublishError, match=r"^Invalid frozen release descriptor$"):
        load_bundle(frozen.root, frozen.ledger, cdn_root="https://cdn.invalid/")
