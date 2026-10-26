"""Prepare fixed public inputs once per run, including xdist workers."""

import pytest

from .author_current_fixtures import REPOSITORY, current_source_inputs
from .prepared_values import freeze, shared_preparation


@pytest.fixture(scope="session")
def author_source_preparation(
    tmp_path_factory: pytest.TempPathFactory, worker_id: str
) -> bytes:
    root = tmp_path_factory.getbasetemp()
    if worker_id != "master":
        root = root.parent
    cache = root / "public-source-preparation"
    cache.mkdir(exist_ok=True)
    return shared_preparation(
        cache / "current-sources.pickle",
        lambda: freeze(
            current_source_inputs(cache / "sources"),
            catalog_root=REPOSITORY / "card-data",
        ),
    )
