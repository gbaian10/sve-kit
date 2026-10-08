import sys
import tempfile
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from .isolation_guard import IsolationGuard
from .r2_sdk_fixtures import close_sdk_clients as close_sdk_clients  # ruff: ignore[useless-import-alias] -- register SDK cleanup fixture

if TYPE_CHECKING:
    from collections.abc import Iterator


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
        guard.protect_filesystem(protection)
        guard.protect_http(protection)
        try:
            yield guard
        finally:
            guard._active = False
