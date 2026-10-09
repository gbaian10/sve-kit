"""Immutable synthetic sources shared by database isolation tests."""

import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.provenance import SourceUse
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import seal_batch

from ..ingest.test_source_archive import NOW, _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture(scope="module")
def sealed_uses_template(
    tmp_path_factory: pytest.TempPathFactory,
) -> tuple[Path, tuple[SourceUse, ...]]:
    store = _store(tmp_path_factory.mktemp("sealed-uses-template") / "frozen")
    raw = b"<html>Synthetic card and product</html>"
    resource = replace(
        _resource("https://example.invalid/card", "raw/card.html", raw, Kind.CARD),
        first_fetched_at=NOW.replace(day=27),
        etag="first-etag",
    )
    _put(store, resource, raw)
    seal_batch(store)
    _put(
        store,
        replace(resource, etag="later-etag", last_checked_at=NOW.replace(day=30)),
        raw,
    )
    sealed = seal_batch(store)
    sources = FrozenSources(store.root, store.store_id, sealed.batch_id)
    version = sources.inventory.current[0].source_version_id
    identity, _, _ = sources.read(version, parser_version="identity-v1")
    product, _, _ = sources.read(version, parser_version="product-v1")
    return store.root, (
        SourceUse(source=identity, usage="registry_observation", locator="card page"),
        SourceUse(
            source=product, usage="product_observation", locator="product block 0"
        ),
    )


@pytest.fixture
def sealed_uses(
    tmp_path: Path, sealed_uses_template: tuple[Path, tuple[SourceUse, ...]]
) -> tuple[Path, tuple[SourceUse, ...]]:
    store, uses = sealed_uses_template
    destination = tmp_path / "frozen/archive"
    shutil.copytree(store, destination)
    return destination, uses
