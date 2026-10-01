from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.manifest import Manifest

from .official_registry_fixtures import load_shared_registry
from .product_fixtures import product_files as product_files  # ruff: ignore[useless-import-alias] -- register session fixture dependency
from .product_identity_fixtures import identity_template as identity_template  # ruff: ignore[useless-import-alias] -- register session fixture dependency
from .registry_snapshot_fixtures import registry_template as registry_template  # ruff: ignore[useless-import-alias] -- register session fixture dependency

if TYPE_CHECKING:
    from sve_carddb.registry.snapshot import RegistrySnapshot
    from sve_carddb.registry.storage import Entry


@pytest.fixture(scope="session")
def official_snapshot(
    tmp_path_factory: pytest.TempPathFactory, worker_id: str
) -> RegistrySnapshot:
    root = Path(__file__).resolve().parents[2] / "authored"
    cache_root = tmp_path_factory.getbasetemp()
    if worker_id != "master":
        cache_root = cache_root.parent
    return load_shared_registry(root, cache_root / "official-registry.json")


@pytest.fixture(scope="session")
def _official_entries(official_snapshot: RegistrySnapshot) -> tuple[Entry, ...]:
    return tuple(record.entry() for record in official_snapshot.records.values())


class FakeClock:
    """A monotonic clock whose sleep only advances time."""

    def __init__(self) -> None:
        self.now = 1000.0
        self.sleeps: list[float] = []

    def __call__(self) -> float:
        return self.now

    async def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.now += seconds


@pytest.fixture
def clock() -> FakeClock:
    return FakeClock()


@pytest.fixture
def manifest(tmp_path: Path) -> Manifest:
    return Manifest.open(tmp_path / "manifest" / "manifest.sqlite")
