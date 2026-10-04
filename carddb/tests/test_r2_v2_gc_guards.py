"""Preview coexistence and incomplete-ledger GC counterexamples."""

from copy import deepcopy
from shutil import copytree
from typing import TYPE_CHECKING

import httpx
import pytest
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.r2_upload.s3 import Credentials
from sve_carddb.r2_upload.v2 import gc
from sve_carddb.snapshot.publish import Ledger, _seal, publish
from sve_carddb.snapshot.publish.plan import IMAGE_CACHE, INDEX
from sve_carddb.snapshot.publish.storage import PublishError, Stored
from sve_carddb.snapshot.values import array, canonical, digest, object_value, string

from .r2_v2_fixtures import remote as remote  # ruff: ignore[useless-import-alias] -- dependency of published
from .r2_v2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- dependency of remote
from .snapshot_publish_fixtures import FakeCDN, FakeS3, candidate, version
from .snapshot_publish_fixtures import images as images  # ruff: ignore[useless-import-alias] -- shared synthetic input
from .test_r2_v2_bundle import frozen as frozen  # ruff: ignore[useless-import-alias] -- dependency of published
from .test_r2_v2_bundle import frozen_base as frozen_base  # ruff: ignore[useless-import-alias] -- dependency of published_base
from .test_r2_v2_gc import NAMESPACES, ORPHAN
from .test_r2_v2_gc import published as published  # ruff: ignore[useless-import-alias] -- isolated publication copies
from .test_r2_v2_gc import published_base as published_base  # ruff: ignore[useless-import-alias] -- immutable remote baseline

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.r2_upload.v2.adapter import R2Store
    from sve_carddb.snapshot.publish import Release

    from .r2_v2_fixtures import Loopback, ServerState
    from .test_r2_v2_bundle import Frozen
    from .test_snapshot_preview_images import PublicImages


def run_gc(
    action: str, frozen: Frozen, store: R2Store, plan: dict[str, JsonValue]
) -> None:
    if action == "inspect":
        gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    else:
        gc.execute(frozen.ledger, store, plan, confirmed=gc.confirmation(plan))


@pytest.mark.parametrize("kind", ["valid", "malformed", "legacy"])
@pytest.mark.parametrize("action", ["inspect", "execute"])
def test_any_preview_pointer_refuses_gc_and_preserves_its_members(
    published: tuple[Frozen, R2Store, ServerState], kind: str, action: str
) -> None:
    frozen, store, state = published
    plan = gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    manifest = deepcopy(frozen.release.snapshot.manifest)
    manifest["data_version"] = "preview-20261004T000000Z-0001"
    raw = canonical(manifest)
    manifest_key = "snapshots/manifests/" + digest(raw)[7:] + ".json"
    state.objects[manifest_key] = Stored(
        raw, '"preview"', {"content-type": "application/json"}
    )
    pointer = canonical({"manifest_path": manifest_key, "manifest_sha256": digest(raw)})
    if kind == "malformed":
        pointer = b"not a valid pointer"
    elif kind == "legacy":
        pointer = canonical(
            {"format_version": "1.0.0", "unknown_manifest": manifest_key}
        )
    state.objects[gc.PREVIEW_POINTER] = Stored(pointer, '"pointer"', {})
    before = deepcopy(state.objects)
    with pytest.raises(
        PublishError, match=r"^GC refuses a bucket with a preview pointer$"
    ):
        run_gc(action, frozen, store, plan)
    assert {
        k: v
        for k, v in state.objects.items()
        if k != "coordination/snapshot-v2-writer.json"
    } == {
        k: v for k, v in before.items() if k != "coordination/snapshot-v2-writer.json"
    }
    assert not any(op[0] == "DELETE" for op in state.operations)


def test_preview_pointer_appearing_after_execute_replan_still_blocks_deletion(
    published: tuple[Frozen, R2Store, ServerState], monkeypatch: pytest.MonkeyPatch
) -> None:
    frozen, store, state = published
    plan = gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    real = gc._unchanged
    checks = 0

    def raced(store: R2Store, approved: Stored) -> None:
        nonlocal checks
        checks += 1
        if checks == 2:
            state.objects[gc.PREVIEW_POINTER] = Stored(b"unknown preview", '"new"', {})
        real(store, approved)

    monkeypatch.setattr(gc, "_unchanged", raced)
    with pytest.raises(
        PublishError, match=r"^GC refuses a bucket with a preview pointer$"
    ):
        gc.execute(frozen.ledger, store, plan, confirmed=gc.confirmation(plan))
    assert checks == 2
    assert ORPHAN in state.objects
    assert not any(op[0] == "DELETE" for op in state.operations)


@pytest.mark.parametrize("action", ["inspect", "execute"])
def test_newer_remote_index_than_local_committed_receipts_blocks_gc(
    published: tuple[Frozen, R2Store, ServerState], images: PublicImages, action: str
) -> None:
    frozen, store, state = published
    plan = gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    paths = (frozen.ledger.path, frozen.ledger.copy, frozen.ledger.receipts)
    old = {p: p.read_bytes() for p in paths}
    origin = FakeS3()
    origin.objects = deepcopy(state.objects)
    next_release = candidate(
        frozen.ledger, images, from_version=version(frozen.release)
    )
    publish(frozen.ledger, origin, next_release, FakeCDN(origin))
    state.objects = deepcopy(origin.objects)
    for path, raw in old.items():
        path.write_bytes(raw)
    frozen.ledger.verify_backup()
    with pytest.raises(
        PublishError, match=r"^GC index differs from durable committed receipts$"
    ):
        run_gc(action, frozen, store, plan)
    assert ORPHAN in state.objects
    assert not any(op[0] == "DELETE" for op in state.operations)


@pytest.mark.parametrize("action", ["inspect", "execute"])
def test_gc_requires_the_matching_durable_backup(
    published: tuple[Frozen, R2Store, ServerState], action: str
) -> None:
    frozen, store, state = published
    plan = gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    frozen.ledger.copy.write_bytes(b"stale backup")
    with pytest.raises(PublishError, match=r"^Release backup is missing or stale$"):
        run_gc(action, frozen, store, plan)
    assert ORPHAN in state.objects
    assert not any(op[0] == "DELETE" for op in state.operations)


@pytest.fixture(scope="module")
def inflight_base(
    images: PublicImages, tmp_path_factory: pytest.TempPathFactory
) -> tuple[Ledger, Release, dict[str, Stored]]:
    root = tmp_path_factory.mktemp("r2-gc-inflight")
    ledger = Ledger(root / "primary", root / "backup")
    ledger.initialize()
    origin = FakeS3()
    hidden = deepcopy(images.projection)
    hidden.tables["image_asset"][0].update(
        publication_state="withdrawn", withdrawal_reason="Synthetic"
    )
    hidden.tables["image_variant"] = []
    first = candidate(ledger, images, projection=hidden)
    publish(ledger, origin, first, FakeCDN(origin))
    staged = candidate(ledger, images, from_version=version(first))
    with ledger.exclusive(), origin.exclusive():
        _seal(ledger, origin, staged)
        for member in staged.members:
            if member.key not in origin.objects:
                assert origin.put(member.key, member.raw, member.headers, expected=None)
        for key, raw in staged.images():
            assert origin.put(
                key,
                raw,
                {"content-type": "image/webp", "cache-control": IMAGE_CACHE},
                expected=None,
            )
    assert object_value(array(ledger.read()["attempts"])[-1])["status"] == "sealed"
    return ledger, staged, deepcopy(origin.objects)


@pytest.mark.parametrize("status", ["sealed", "failed"])
def test_inflight_members_and_images_are_retained_without_current_references(
    inflight_base: tuple[Ledger, Release, dict[str, Stored]],
    remote: tuple[R2Store, ServerState, Loopback],
    tmp_path: Path,
    status: str,
) -> None:
    base, release, objects = inflight_base
    copytree(base.root, tmp_path / "primary")
    copytree(base.backup, tmp_path / "backup")
    ledger = Ledger(tmp_path / "primary", tmp_path / "backup")
    origin = FakeS3()
    origin.objects = deepcopy(objects)
    if status == "failed":

        def fail_index(key: str) -> None:
            if key == INDEX:
                raise PublishError("Synthetic index outage")

        origin.before_put = fail_index
        with pytest.raises(PublishError, match=r"^Synthetic index outage$"):
            publish(ledger, origin, release, FakeCDN(origin))
    assert object_value(array(ledger.read()["attempts"])[-1])["status"] == status
    store, state, _transport = remote
    state.objects = deepcopy(origin.objects)
    state.objects[ORPHAN] = Stored(b"orphan", '"orphan"', {})
    namespaces = NAMESPACES | frozenset(
        {"images/card_m/", "images/card_l/", "images/art_s/", "images/art_m/"}
    )
    before = deepcopy(state.objects)
    plan = gc.inspect(ledger, store, namespaces=namespaces)
    assert gc.report(plan)["would_collect"] == [ORPHAN]
    assert gc.execute(ledger, store, plan, confirmed=gc.confirmation(plan)) == (ORPHAN,)
    assert all(state.objects[m.key] == before[m.key] for m in release.members)
    assert all(
        state.objects[string(a["path"])] == before[string(a["path"])]
        for a in release.assets
    )


@pytest.mark.parametrize("action", ["inspect", "execute", "both"])
def test_gc_cli_authorization_is_separate_from_the_exact_confirmation(
    published: tuple[Frozen, R2Store, ServerState],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    action: str,
) -> None:
    frozen, store, state = published
    plan = gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    path = tmp_path / "gc.json"
    if action != "inspect":
        path.write_bytes(canonical(plan))

    def forbidden(*_args: object, **_kwargs: object) -> None:
        pytest.fail("unauthorized GC accessed credentials or HTTP")

    monkeypatch.setattr(Credentials, "environment", forbidden)
    monkeypatch.setattr(httpx, "Client", forbidden)
    args = [
        "r2",
        "gc-v2",
        "--plan-file",
        str(path),
        "--ledger-dir",
        str(frozen.ledger.root),
        "--backup-dir",
        str(frozen.ledger.backup),
        "--checkpoint-file",
        str(frozen.checkpoint),
    ]
    if action in {"inspect", "both"}:
        args += ["--inspect-remote", "--namespace", "snapshots/blobs/"]
    if action in {"execute", "both"}:
        args += ["--execute", "--confirm-delete", gc.confirmation(plan)]
    result = CliRunner().invoke(app, args, env={"COLUMNS": "200"})
    assert result.exit_code == 2, result.output
    expected = {
        "inspect": "Remote GC inspection requires explicit authorization",
        "execute": "GC execution requires authorization",
        "both": "GC inspection and deletion must be separate actions",
    }
    assert expected[action] in result.output
    assert not any(op[0] == "DELETE" for op in state.operations)
