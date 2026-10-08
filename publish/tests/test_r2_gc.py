"""GC keeps the remote current/previous JSON closures and current images only."""

import json
from copy import deepcopy
from typing import TYPE_CHECKING

import httpx
import pytest
from sve_carddb.export.read_api import INDEX, ExportError, load_export
from typer.testing import CliRunner

from sve_publish import gc
from sve_publish.adapter import PUBLIC_PREFIXES, Stored
from sve_publish.cli import app
from sve_publish.headers import IMAGE_HEADERS
from sve_publish.publish import upload

from .r2_export_fixtures import export
from .r2_export_fixtures import images as images  # ruff: ignore[useless-import-alias] -- module-scoped synthetic corpus
from .r2_export_fixtures import roots as roots  # ruff: ignore[useless-import-alias] -- per-test public and private roots
from .r2_fixtures import ACCOUNT, BUCKET
from .r2_fixtures import remote as remote  # ruff: ignore[useless-import-alias] -- isolated loopback server
from .r2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- dependency of remote
from .r2_sdk_fixtures import install_mock_sdk

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
        upload(store, loaded[-1], None)
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
    expected = sorted((keys(first) - keys(second) - keys(third)) | {STRAY})
    assert expected
    dry = gc.collect(store, PUBLIC_PREFIXES, execute=False)
    assert dry == {"mode": "dry_run", "candidates": expected, "deleted": []}
    assert STRAY in state.objects
    done = gc.collect(store, PUBLIC_PREFIXES, execute=True)
    assert done["deleted"] == expected
    assert set(state.objects) == keys(second) | keys(third) | {INDEX}
    assert gc.collect(store, PUBLIC_PREFIXES, execute=False)["candidates"] == []


def test_namespace_limits_candidates(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    three_versions(images, roots, store)
    state.objects[STRAY] = Stored(b"stray", '"stray"', IMAGE_HEADERS)
    result = gc.collect(store, frozenset({"images/card_s/"}), execute=True)
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
    upload(store, previous, None)
    absent = deepcopy(images.projection)
    for row in absent.tables["printing_image"]:
        row["availability"] = "missing"
    absent.tables["image_variant"] = []
    export(images, roots, step=1, projection=absent)
    current = load_export(roots.preview)
    assert not current.images
    upload(store, current, None)
    result = gc.collect(store, PUBLIC_PREFIXES, execute=True)
    assert result["deleted"] == sorted(i.key for i in previous.images)
    assert set(state.objects) == {
        m.key for e in (previous, current) for m in e.members
    } | {INDEX}


def test_missing_retained_member_stops_before_deletion(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    _first, second, _third = three_versions(images, roots, store)
    state.objects[STRAY] = Stored(b"stray", '"stray"', IMAGE_HEADERS)
    del state.objects[second.members[-1].key]
    with pytest.raises(ExportError, match=r"^GC retained closure is incomplete$"):
        gc.collect(store, PUBLIC_PREFIXES, execute=True)
    assert STRAY in state.objects


def test_index_change_stops_deletion(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    store, state, _ = remote
    three_versions(images, roots, store)
    for number in range(2):
        state.objects[f"images/card_s/99{number}.webp"] = Stored(
            b"stray", '"stray"', IMAGE_HEADERS
        )
    original = store.delete

    def delete_then_change(key: str) -> None:
        original(key)
        old = state.objects[INDEX]
        state.objects[INDEX] = Stored(old.raw, '"changed"', old.headers)

    monkeypatch.setattr(store, "delete", delete_then_change)
    with pytest.raises(ExportError, match=r"^GC version index changed; run it again$"):
        gc.collect(store, PUBLIC_PREFIXES, execute=True)
    assert len([k for k in state.objects if k.startswith("images/card_s/99")]) == 1


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
        state.objects["images/card_s/stray.png"] = Stored(b"x", '"x"', IMAGE_HEADERS)
        message = "GC inventory contains an irregular public key"
    with pytest.raises(ExportError, match="^" + message + "$"):
        gc.collect(store, PUBLIC_PREFIXES, execute=True)
    assert not any(op == "DELETE" for op, _key, _ in state.operations)


def test_cli_dry_run_lists_and_execute_deletes(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
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
    common = ["gc", "--namespace", "images/card_s/", *target]
    dry = CliRunner().invoke(app, common)
    assert dry.exit_code == 0, dry.output
    assert json.loads(dry.output)["candidates"] == [STRAY]
    assert STRAY in state.objects
    done = CliRunner().invoke(app, [*common, "--execute"])
    assert done.exit_code == 0, done.output
    assert json.loads(done.output)["deleted"] == [STRAY]
    assert STRAY not in state.objects
