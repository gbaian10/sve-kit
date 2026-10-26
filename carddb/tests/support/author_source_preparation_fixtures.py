"""Prepare fixed public inputs once per run, including xdist workers."""

import pytest

from .author_current_fixtures import REPOSITORY, current_source_inputs
from .prepared_values import freeze, shared_preparation
from .time_budget import slow_fixture


@pytest.fixture(scope="session")
@slow_fixture(
    reason="Prepare fixed real public sources once across xdist workers; the other workers wait for the same immutable cache."
)
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
