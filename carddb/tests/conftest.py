import sys
import tempfile
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.manifest import Manifest

from .database_fixtures import (
    image_parent_database_template as image_parent_database_template,  # ruff: ignore[useless-import-alias] -- register shared immutable database template
)
from .database_fixtures import t0_database_template as t0_database_template  # ruff: ignore[useless-import-alias] -- register shared immutable database template
from .database_fixtures import t1_database_template as t1_database_template  # ruff: ignore[useless-import-alias] -- register shared immutable database template
from .database_fixtures import t1b_database_template as t1b_database_template  # ruff: ignore[useless-import-alias] -- register shared immutable database template
from .image_archive_fixtures import image_archive_template as image_archive_template  # ruff: ignore[useless-import-alias] -- register shared synthetic archive template
from .image_crop_fixtures import empty_crops as empty_crops  # ruff: ignore[useless-import-alias] -- register shared pinned empty crop closure
from .isolation_guard import IsolationGuard
from .official_registry_fixtures import load_shared_registry
from .product_fixtures import product_files as product_files  # ruff: ignore[useless-import-alias] -- register session fixture dependency
from .product_identity_fixtures import identity_template as identity_template  # ruff: ignore[useless-import-alias] -- register session fixture dependency
from .registry_snapshot_fixtures import registry_template as registry_template  # ruff: ignore[useless-import-alias] -- register session fixture dependency
from .shared_case_fixtures import default_correction_case as default_correction_case  # ruff: ignore[useless-import-alias] -- register session fixture dependency
from .shared_case_fixtures import default_text_case as default_text_case  # ruff: ignore[useless-import-alias] -- register session fixture dependency

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.registry.snapshot import RegistrySnapshot
    from sve_carddb.registry.storage import Entry


@pytest.fixture(scope="session", autouse=True)  # ruff: ignore[pytest-fixture-autouse] -- safety must cover every test and session fixture
def test_isolation_guard(
    tmp_path_factory: pytest.TempPathFactory, worker_id: str
) -> Iterator[IsolationGuard]:
    temporary = tmp_path_factory.getbasetemp()
    if worker_id != "master":
        temporary = temporary.parent
    coverage = Path(__file__).resolve().parents[1] / "htmlcov"
    standard_temporary = tmp_path_factory.mktemp("stdlib-temp")
    guard = IsolationGuard(temporary, coverage)
    sys.addaudithook(guard.audit)
    # This patch belongs to the session, so a test's undo cannot release it.
    with pytest.MonkeyPatch.context() as protection:
        protection.setenv("SVE_DATA_DIR", str(temporary / "isolated-default-data"))
        protection.setenv("TMPDIR", str(standard_temporary))
        protection.setattr(tempfile, "tempdir", str(standard_temporary))
        guard.protect_sources(protection)
        guard.protect_http(protection)
        try:
            yield guard
        finally:
            guard._active = False


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
