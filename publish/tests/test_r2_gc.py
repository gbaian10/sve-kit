"""GC keeps the remote current/previous JSON closures and current images only."""

import json
from copy import deepcopy
from typing import TYPE_CHECKING

import httpx
import pytest
from sve_carddb.export.read_api import (
    INDEX,
    ExportError,
    canonical,
    closure,
    digest,
    load_export,
    object_value,
    parse,
    string,
)
from typer.testing import CliRunner

from sve_publish import gc
from sve_publish.adapter import (
    IMAGE_PREFIXES,
    MAX_DELETE_BATCH,
    SNAPSHOT_PREFIXES,
    Stored,
)
from sve_publish.cli import app
from sve_publish.headers import IMAGE_HEADERS, INDEX_HEADERS, JSON_HEADERS
from sve_publish.inventory import INVENTORY
from sve_publish.snapshot_diff import missing_members

from .r2_export_fixtures import export
from .r2_export_fixtures import images as images  # ruff: ignore[useless-import-alias] -- module-scoped synthetic corpus
from .r2_export_fixtures import roots as roots  # ruff: ignore[useless-import-alias] -- per-test public and private roots
from .r2_fixtures import ACCOUNT, BUCKET
from .r2_fixtures import remote as remote  # ruff: ignore[useless-import-alias] -- isolated loopback server
from .r2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- dependency of remote
from .r2_sdk_fixtures import install_mock_sdk
from .test_r2_upload import upload

pytestmark = pytest.mark.usefixtures("close_sdk_clients")

if TYPE_CHECKING:
    from sve_carddb.export.preview import Roots
    from sve_carddb.export.read_api import Export

    from sve_publish.adapter import R2Store

    from .r2_fixtures import Loopback, ServerState
    from .synthetic_images import PublicImages

STRAY = "images/card_s/999.webp"


def three_versions(
    images: PublicImages, roots: Roots, store: R2Store
) -> tuple[Export, ...]:
    """Upload three text-only versions so the first one leaves the window."""
    loaded = []
    for step in range(3):
        projection = deepcopy(images.projection)
        projection.tables["printing"][0]["rarity_raw"] = f"Synthetic rarity {step}"
        export(images, roots, step=step, projection=projection)
        loaded.append(load_export(roots.preview))
        upload(store, loaded[-1])
    return tuple(loaded)


def keys(loaded: Export) -> set[str]:
    return {m.key for m in loaded.members} | {i.key for i in loaded.images}


def test_collects_only_objects_outside_current_previous_and_current_images(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    first, second, third = three_versions(images, roots, store)
    state.objects[STRAY] = Stored(b"stray", '"stray"', IMAGE_HEADERS)
    expected = sorted(keys(first) - keys(second) - keys(third))
    assert expected
    dry = gc.collect(store, SNAPSHOT_PREFIXES, execute=False)
    assert dry == {"mode": "dry_run", "candidates": expected, "deleted": []}
    assert STRAY in state.objects
    done = gc.collect(store, SNAPSHOT_PREFIXES, execute=True)
    assert done["deleted"] == expected
    assert STRAY in state.objects
    images_done = gc.collect_images(store, IMAGE_PREFIXES, execute=True)
    assert images_done["deleted"] == [STRAY]
    assert set(state.objects) == keys(second) | keys(third) | {INDEX, INVENTORY}
    assert gc.collect(store, SNAPSHOT_PREFIXES, execute=False)["candidates"] == []


def test_namespace_limits_candidates(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    three_versions(images, roots, store)
    state.objects[STRAY] = Stored(b"stray", '"stray"', IMAGE_HEADERS)
    result = gc.collect_images(store, frozenset({"images/card_s/"}), execute=True)
    assert result["deleted"] == [STRAY]
    with pytest.raises(ExportError, match=r"^GC namespace is not explicitly public$"):
        gc.collect(store, frozenset({"snapshots/"}), execute=False)


def test_previous_images_are_collected_when_current_drops_them(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    previous = load_export(roots.preview)
    upload(store, previous)
    absent = deepcopy(images.projection)
    for row in absent.tables["printing_image"]:
        row["availability"] = "missing"
    absent.tables["image_variant"] = []
    export(images, roots, step=1, projection=absent)
    current = load_export(roots.preview)
    assert not current.images
    upload(store, current)
    result = gc.collect_images(store, IMAGE_PREFIXES, execute=True)
    assert result["deleted"] == sorted(i.key for i in previous.images)
    assert set(state.objects) == {
        m.key for e in (previous, current) for m in e.members
    } | {INDEX, INVENTORY}


def test_missing_retained_member_stops_before_deletion(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    _first, second, third = three_versions(images, roots, store)
    state.objects[STRAY] = Stored(b"stray", '"stray"', IMAGE_HEADERS)
    current_members = {m.key for m in third.members}
    missing = next(
        m.key
        for m in second.members
        if m.key.startswith("snapshots/blobs/") and m.key not in current_members
    )
    del state.objects[missing]
    with pytest.raises(ExportError, match=r"^GC retained closure is incomplete$"):
        gc.collect(store, SNAPSHOT_PREFIXES, execute=True)
    assert STRAY in state.objects


@pytest.mark.parametrize("kind", ["snapshot", "images"])
@pytest.mark.parametrize("change_index", [False, True])
def test_batches_delete_and_stop_if_index_changes(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    kind: str,
    change_index: bool,
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    upload(store, load_export(roots.preview))
    state.page_size = 1000
    collector = gc.collect if kind == "snapshot" else gc.collect_images
    namespaces = SNAPSHOT_PREFIXES if kind == "snapshot" else IMAGE_PREFIXES
    keys = {
        f"snapshots/blobs/{number:064x}.json"
        if kind == "snapshot"
        else f"images/card_s/{10000 + number}.webp"
        for number in range(MAX_DELETE_BATCH * 2 + MAX_DELETE_BATCH // 2)
    }
    state.objects.update({key: Stored(b"stray", '"stray"', {}) for key in keys})
    candidates = collector(store, namespaces, execute=False)["candidates"]
    assert isinstance(candidates, list)
    before = set(state.objects)
    progress: list[str] = []
    if change_index:
        old = state.objects[INDEX]
        state.index_after_delete = Stored(old.raw, '"changed"', old.headers)
        with pytest.raises(
            ExportError, match=r"^GC version index changed; run it again$"
        ):
            collector(store, namespaces, execute=True, progress=progress.append)
        deleted = candidates[:MAX_DELETE_BATCH]
        assert [len(batch) for batch in state.delete_batches] == [MAX_DELETE_BATCH]
        assert progress == [
            f"GC: {len(candidates)} candidates",
            f"GC: {MAX_DELETE_BATCH}/{len(candidates)} deleted",
        ]
    else:
        result = collector(store, namespaces, execute=True, progress=progress.append)
        deleted = candidates
        assert result["deleted"] == candidates
        assert [len(batch) for batch in state.delete_batches] == [
            len(batch)
            for batch in (
                candidates[start : start + MAX_DELETE_BATCH]
                for start in range(0, len(candidates), MAX_DELETE_BATCH)
            )
        ]
        assert progress == [
            f"GC: {len(candidates)} candidates",
            *(
                f"GC: {min(start + MAX_DELETE_BATCH, len(candidates))}/{len(candidates)} deleted"
                for start in range(0, len(candidates), MAX_DELETE_BATCH)
            ),
        ]
    assert set(state.objects) == before - set(deleted)
    state.index_after_delete = None
    assert (
        collector(store, namespaces, execute=True)["deleted"]
        == candidates[len(deleted) :]
    )


@pytest.mark.parametrize("case", ["no-index", "irregular"])
def test_refuses_bucket_without_index_or_with_irregular_keys(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    case: str,
) -> None:
    store, state, _ = remote
    if case == "no-index":
        message = "GC requires a published version index"
    else:
        three_versions(images, roots, store)
        state.objects["snapshots/blobs/stray.png"] = Stored(b"x", '"x"', IMAGE_HEADERS)
        message = "GC inventory contains an irregular public key"
    with pytest.raises(ExportError, match="^" + message + "$"):
        gc.collect(store, SNAPSHOT_PREFIXES, execute=True)
    assert not any(op == "DELETE" for op, _key, _ in state.operations)


@pytest.mark.parametrize("kind", ["snapshot", "images"])
def test_cli_dry_run_lists_and_execute_deletes(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
    kind: str,
) -> None:
    store, state, transport = remote
    three_versions(images, roots, store)
    state.objects[STRAY] = Stored(b"stray", '"stray"', IMAGE_HEADERS)
    real_client = httpx.Client

    def factory(**_kwargs: object) -> httpx.Client:
        return real_client(transport=transport, trust_env=False, follow_redirects=False)

    install_mock_sdk(monkeypatch, transport)
    monkeypatch.setattr(httpx, "Client", factory)
    monkeypatch.setenv("SVE_R2_ACCESS_KEY_ID", "synthetic-access")
    monkeypatch.setenv("SVE_R2_SECRET_ACCESS_KEY", "synthetic-secret")
    target = ["--account-id", ACCOUNT, "--bucket", BUCKET]
    namespace = "images/card_s/" if kind == "images" else "snapshots/blobs/"
    common = [kind, "gc", "--namespace", namespace, *target]
    dry = CliRunner().invoke(app, common)
    assert dry.exit_code == 0, dry.output
    candidates = json.loads(dry.stdout)["candidates"]
    assert candidates
    assert dry.stderr == f"GC: {len(candidates)} candidates\n"
    assert STRAY in state.objects
    done = CliRunner().invoke(app, [*common, "--execute"])
    assert done.exit_code == 0, done.output
    assert json.loads(done.stdout)["deleted"] == candidates
    assert done.stderr == (
        f"GC: {len(candidates)} candidates\n"
        f"GC: {len(candidates)}/{len(candidates)} deleted\n"
    )
    assert not set(candidates).intersection(state.objects)
    empty = CliRunner().invoke(app, [*common, "--execute"])
    assert empty.exit_code == 0, empty.output
    assert json.loads(empty.stdout)["deleted"] == []
    assert empty.stderr == "GC: 0 candidates\n"


@pytest.mark.parametrize("slot", ["current", "previous"])
def test_diff_and_gc_accept_retained_reader_4_0_0_manifests(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    slot: str,
) -> None:
    store, state, _ = remote
    _first, _second, current = three_versions(images, roots, store)
    index = object_value(parse(state.objects[INDEX].raw))
    entry = object_value(index[slot])
    manifest = object_value(parse(state.objects[string(entry["manifest_path"])].raw))
    manifest["min_reader_version"] = "4.0.0"
    raw = canonical(manifest)
    path = "snapshots/manifests/" + digest(raw)[7:] + ".json"
    entry.update(
        min_reader_version="4.0.0", manifest_path=path, manifest_sha256=digest(raw)
    )
    state.objects[path] = Stored(raw, '"legacy-minimum"', JSON_HEADERS)
    state.objects[INDEX] = Stored(canonical(index), '"mixed-window"', INDEX_HEADERS)

    missing = missing_members(store, index, current.members)
    expected = {string(current.entry["manifest_path"])} if slot == "current" else set()
    assert {member.key for member in missing} == expected
    result = gc.collect(store, SNAPSHOT_PREFIXES, execute=False)
    candidates = result["candidates"]
    assert isinstance(candidates, list)
    assert not set(candidates).intersection(closure(path, manifest))
    assert gc.collect_images(store, IMAGE_PREFIXES, execute=False)["candidates"] == []
    assert not any(operation == "DELETE" for operation, _key, _ in state.operations)
