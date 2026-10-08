"""One synthetic image archive per worker, copied by mutating tests."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.ingest.archive.source_archive import Scope, seal_batch

from .test_image_variants import png
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture(scope="session")
def image_archive_template(
    tmp_path_factory: pytest.TempPathFactory,
) -> tuple[Path, str, str]:
    store = _store(tmp_path_factory.mktemp("image-archive-template") / "input")
    for index, dimensions in enumerate(((80, 112), (112, 80), (80, 112))):
        data = png(*dimensions)
        _put(
            store,
            _resource(
                f"https://shadowverse-evolve.com/synthetic/{index}.png",
                f"raw/{index}.png",
                data,
            ),
            data,
        )
    batch = seal_batch(store, scope=(Scope(provider="jp", kind="image"),))
    return store.root, store.store_id, batch.batch_id
