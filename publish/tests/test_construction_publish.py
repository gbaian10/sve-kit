"""Construction indexes travel with their snapshot and are never replaced in place."""

from copy import deepcopy
from typing import TYPE_CHECKING

import pytest
from sve_carddb.export.read_api import (
    ExportError,
    canonical,
    load_export,
    object_value,
    parse,
)

from sve_publish import gc
from sve_publish.adapter import SNAPSHOT_PREFIXES, Stored
from sve_publish.headers import JSON_HEADERS

from .r2_export_fixtures import export
from .r2_export_fixtures import images as images  # ruff: ignore[useless-import-alias] -- module-scoped synthetic corpus
from .r2_export_fixtures import roots as roots  # ruff: ignore[useless-import-alias] -- per-test public and private roots
from .r2_fixtures import remote as remote  # ruff: ignore[useless-import-alias] -- isolated loopback server
from .r2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- dependency of remote
from .test_r2_upload import upload

pytestmark = pytest.mark.usefixtures("close_sdk_clients")

if TYPE_CHECKING:
    from sve_carddb.export.preview import Roots

    from sve_publish.adapter import R2Store

    from .r2_fixtures import Loopback, ServerState
    from .synthetic_images import PublicImages


def test_an_export_needs_its_own_construction_index(
    images: PublicImages, roots: Roots
) -> None:
    export(images, roots, step=0)
    [member] = load_export(roots.preview).construction
    path = roots.preview / member.key
    path.unlink()
    with pytest.raises(ExportError, match="lacks a construction index"):
        load_export(roots.preview)
    other = object_value(parse(member.raw)) | {"region": "en"}
    path.write_bytes(canonical(other))
    with pytest.raises(ExportError, match="differs from its snapshot"):
        load_export(roots.preview)


@pytest.mark.parametrize("mismatch", ["bytes", "headers"])
def test_rerun_completes_a_missing_index_but_never_replaces_one(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    mismatch: str,
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    upload(store, loaded)
    [member] = loaded.construction
    del state.objects[member.key]
    again = upload(store, loaded)
    assert again["index"] == "unchanged"
    assert again["written_files"] == 1
    assert state.objects[member.key].raw == member.raw
    state.objects[member.key] = Stored(
        b"{}" if mismatch == "bytes" else member.raw,
        '"other"',
        JSON_HEADERS
        if mismatch == "bytes"
        else JSON_HEADERS | {"cache-control": "no-store"},
    )
    before = dict(state.objects)
    state.requests.clear()
    with pytest.raises(ExportError, match="Stored construction index differs"):
        upload(store, loaded)
    assert state.objects == before
    assert all(method == "GET" for method, _key, _headers in state.requests)


@pytest.mark.parametrize("missing", ["none", "current", "previous", "both"])
def test_gc_keeps_current_and_previous_indexes_only(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    missing: str,
) -> None:
    store, state, _ = remote
    keys = []
    for step in range(3):
        projection = deepcopy(images.projection)
        projection.tables["printing"][0]["rarity_raw"] = f"Synthetic rarity {step}"
        export(images, roots, step=step, projection=projection)
        loaded = load_export(roots.preview)
        upload(store, loaded)
        keys.append(loaded.construction[0].key)
    # Missing retained indexes also represent snapshots published before indexes existed.
    absent = set()
    if missing in {"previous", "both"}:
        absent.add(keys[1])
    if missing in {"current", "both"}:
        absent.add(keys[2])
    for key in absent:
        del state.objects[key]
    result = gc.collect(store, SNAPSHOT_PREFIXES, execute=True)
    deleted = result["deleted"]
    assert isinstance(deleted, list)
    assert keys[0] in deleted
    assert not set(keys[1:]) & set(deleted)
    for key in keys[1:]:
        assert (key in state.objects) == (key not in absent)
    assert keys[0] not in state.objects
