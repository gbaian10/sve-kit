"""Keep fixture-heavy files together while spreading independent fresh imports."""

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
    node.workerinput["sve_file_groups"] = (
        node.config.getoption("dist", default="no") == "loadgroup"
    )


@pytest.hookimpl(tryfirst=True)
def pytest_collection_modifyitems(
    config: pytest.Config, items: list[pytest.Item]
) -> None:
    if getattr(config, "workerinput", {}).get("sve_file_groups") is not True:
        return
    imports = config.rootpath / "tests/architecture/test_module_imports.py"
    # xdist must see these public marks before it constructs its group node IDs.
    for item in items:
        if item.path != imports:
            item.add_marker(
                pytest.mark.xdist_group(
                    item.path.relative_to(config.rootpath).as_posix()
                )
            )
    # Case counts understate real-repository setup costs.
    priority = {
        config.rootpath / name: index
        for index, name in enumerate(
            (
                "tests/workflows/test_authoring.py",
                "tests/workflows/test_authoring_cli.py",
            )
        )
    }
    counts = Counter(item.path for item in items)
    items.sort(
        key=lambda item: (priority.get(item.path, len(priority)), -counts[item.path])
    )
