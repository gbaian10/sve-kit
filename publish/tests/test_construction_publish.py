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


def test_rerun_completes_a_missing_index_but_never_replaces_one(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
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
    state.objects[member.key] = Stored(b"{}", '"other"', JSON_HEADERS)
    with pytest.raises(ExportError, match="Stored construction index differs"):
        upload(store, loaded)


def test_gc_keeps_current_and_previous_indexes_only(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
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
    # The second index is gone already, as for a snapshot published before indexes existed.
    del state.objects[keys[1]]
    result = gc.collect(store, SNAPSHOT_PREFIXES, execute=True)
    deleted = result["deleted"]
    assert isinstance(deleted, list)
    assert keys[0] in deleted
    assert keys[2] in state.objects
    assert keys[0] not in state.objects
