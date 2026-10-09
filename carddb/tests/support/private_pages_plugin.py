import os
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- pluggy evaluates hook annotations at registration

import pytest
from _pytest.terminal import TerminalReporter  # ruff: ignore[typing-only-third-party-import] -- pluggy evaluates hook annotations at registration

from .private_pages_support import PrivatePageError, mode

MODE = pytest.StashKey[str]()


def pytest_configure(config: pytest.Config) -> None:
    try:
        config.stash[MODE] = mode(os.environ)
    except PrivatePageError as exc:
        raise pytest.UsageError(str(exc)) from None
    config.addinivalue_line(
        "markers", "private_pages: needs pinned private official pages"
    )


def pytest_ignore_collect(collection_path: Path, config: pytest.Config) -> bool | None:
    if (
        collection_path.name == "test_private_official_pages.py"
        and config.stash[MODE] == "excluded"
    ):
        return True
    return None


def pytest_collection_modifyitems(
    config: pytest.Config, items: list[pytest.Item]
) -> None:
    if config.stash[MODE] == "excluded":
        excluded = [item for item in items if item.get_closest_marker("private_pages")]
        if excluded:
            items[:] = [
                item for item in items if not item.get_closest_marker("private_pages")
            ]
            config.hook.pytest_deselected(items=excluded)


def pytest_terminal_summary(terminalreporter: TerminalReporter) -> None:
    if terminalreporter.config.stash[MODE] == "excluded":
        terminalreporter.write_line("私有真實頁測試未執行")
