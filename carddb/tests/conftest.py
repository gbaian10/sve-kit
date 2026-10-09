import sys
import tempfile
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from rich.console import Console
from typer import rich_utils

from sve_carddb import cli
from sve_carddb.ingest.archive.manifest import Manifest

from .support.database_fixtures import (
    image_parent_database_template as image_parent_database_template,  # ruff: ignore[useless-import-alias] -- register shared immutable database template
)
from .support.database_fixtures import t0_database_template as t0_database_template  # ruff: ignore[useless-import-alias] -- register shared immutable database template
from .support.database_fixtures import t1_database_template as t1_database_template  # ruff: ignore[useless-import-alias] -- register shared immutable database template
from .support.database_fixtures import t1b_database_template as t1b_database_template  # ruff: ignore[useless-import-alias] -- register shared immutable database template
from .support.image_archive_fixtures import (
    image_archive_template as image_archive_template,  # ruff: ignore[useless-import-alias] -- register shared synthetic archive template
)
from .support.image_crop_fixtures import empty_crops as empty_crops  # ruff: ignore[useless-import-alias] -- register shared empty crop set
from .support.isolation_guard import IsolationGuard
from .support.official_registry_fixtures import load_shared_registry
from .support.product_fixtures import product_files as product_files  # ruff: ignore[useless-import-alias] -- register session fixture dependency
from .support.product_identity_fixtures import identity_template as identity_template  # ruff: ignore[useless-import-alias] -- register session fixture dependency
from .support.registry_snapshot_fixtures import registry_template as registry_template  # ruff: ignore[useless-import-alias] -- register session fixture dependency
from .support.shared_case_fixtures import (
    default_correction_case as default_correction_case,  # ruff: ignore[useless-import-alias] -- register session fixture dependency
)
from .support.shared_case_fixtures import default_text_case as default_text_case  # ruff: ignore[useless-import-alias] -- register session fixture dependency

pytest_plugins = ("tests.support.private_pages_plugin",)

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.domains.registry.snapshot import RegistrySnapshot
    from sve_carddb.domains.registry.storage import Entry


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
        protection.delenv("SVE_EXPORT_DIR", raising=False)
        protection.delenv("SVE_CARDDB_PRIVATE_DIR", raising=False)
        protection.setenv("TMPDIR", str(standard_temporary))
        protection.setattr(tempfile, "tempdir", str(standard_temporary))
        guard.protect_sources(protection)
        guard.protect_http(protection)
        try:
            yield guard
        finally:
            guard._active = False


@pytest.fixture(autouse=True)  # ruff: ignore[pytest-fixture-autouse] -- every CLI assertion needs escape-free output
def plain_cli_output(monkeypatch: pytest.MonkeyPatch) -> None:
    """Make CLI output plain text; a test that checks color overrides this itself."""
    monkeypatch.setenv("NO_COLOR", "1")
    monkeypatch.setenv("TERM", "dumb")
    monkeypatch.delenv("FORCE_COLOR", raising=False)
    monkeypatch.delenv("PY_COLORS", raising=False)
    # Typer fixes this at import from FORCE_COLOR and GITHUB_ACTIONS, so the env above is too late.
    monkeypatch.setattr(rich_utils, "FORCE_TERMINAL", None)
    # The imported console has already cached the caller's color mode.
    monkeypatch.setattr(cli, "console", Console(soft_wrap=True, color_system=None))


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
