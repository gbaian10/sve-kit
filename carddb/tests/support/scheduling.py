"""Start the known expensive integration files before the small cheap cases."""

from __future__ import annotations

from collections import Counter
from typing import Protocol

import pytest


class WorkerNode(Protocol):
    config: pytest.Config
    workerinput: dict[str, object]


@pytest.hookimpl(optionalhook=True)
def pytest_configure_node(node: WorkerNode) -> None:
    # xdist resets workers' distribution options before they collect tests.
    node.workerinput["sve_prioritize_files"] = (
        node.config.getoption("dist", default="no") == "loadfile"
    )


def pytest_collection_modifyitems(
    config: pytest.Config, items: list[pytest.Item]
) -> None:
    if getattr(config, "workerinput", {}).get("sve_prioritize_files") is not True:
        return
    # Case counts understate real-repository setup and fresh-process import costs.
    priority = {
        config.rootpath / name: index
        for index, name in enumerate(
            (
                "tests/architecture/test_module_imports.py",
                "tests/workflows/test_authoring.py",
                "tests/workflows/test_authoring_cli.py",
            )
        )
    }
    counts = Counter(item.path for item in items)
    items.sort(
        key=lambda item: (priority.get(item.path, len(priority)), -counts[item.path])
    )
