"""Run one slice of the collected tests so CI can split the suite across jobs.

Every test belongs to exactly one shard, dealt out from its file's sorted node IDs, so
the slices never overlap and a new test needs no registration. The report lets the
coverage job check that the shards together ran every collected test.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
from pathlib import Path

import pytest

SHARD = re.compile(r"([1-9][0-9]*)/([1-9][0-9]*)")


def pytest_addoption(parser: pytest.Parser) -> None:
    group = parser.getgroup("sve-shard", "deterministic test sharding")
    group.addoption(
        "--shard",
        metavar="I/N",
        help="Run only shard I of N (1-based); every test is in exactly one shard.",
    )
    group.addoption(
        "--shard-report",
        metavar="PATH",
        help="Write the collected and selected counts of this shard as JSON.",
    )


def parse_shard(value: str) -> tuple[int, int]:
    """Return the 1-based shard index and the shard count."""
    match = SHARD.fullmatch(value)
    if match is None or int(match[1]) > int(match[2]):
        msg = f"--shard must be I/N with 1 <= I <= N, not {value!r}"
        raise pytest.UsageError(msg)
    return int(match[1]), int(match[2])


def shard_of(nodeid: str, total: int) -> int:
    """Stable across processes and machines, unlike the salted built-in hash."""
    digest = hashlib.sha256(nodeid.encode()).digest()
    return int.from_bytes(digest[:8], "big") % total + 1


def assign(nodeids: list[str], total: int) -> dict[str, int]:
    """Deal each file's tests round-robin, starting at a shard chosen by the file name.

    A few real-repository tests take minutes and sit together in a handful of files;
    dealing a file's tests out in turn spreads them over the shards, where hashing each
    test alone can stack several in one shard.
    """
    files: dict[str, list[str]] = {}
    for nodeid in nodeids:
        files.setdefault(nodeid.split("::", 1)[0], []).append(nodeid)
    shards: dict[str, int] = {}
    for name, tests in files.items():
        start = shard_of(name, total) - 1
        for offset, nodeid in enumerate(sorted(tests)):
            shards[nodeid] = (start + offset) % total + 1
    return shards


@pytest.hookimpl(trylast=True)
def pytest_collection_modifyitems(
    config: pytest.Config, items: list[pytest.Item]
) -> None:
    value = config.getoption("shard")
    if value is None:
        return
    index, total = parse_shard(value)
    shards = assign([item.nodeid for item in items], total)
    selected: list[pytest.Item] = []
    deselected: list[pytest.Item] = []
    for item in items:
        (selected if shards[item.nodeid] == index else deselected).append(item)
    report = config.getoption("shard_report")
    if report is not None:
        path = Path(report)
        path.parent.mkdir(parents=True, exist_ok=True)
        # Every xdist worker collects the same items; replace atomically so readers see one whole file.
        partial = path.with_name(f"{path.name}.{os.getpid()}")
        partial.write_text(
            json.dumps(
                {
                    "index": index,
                    "total": total,
                    "collected": len(items),
                    "selected": len(selected),
                }
            ),
            encoding="utf-8",
        )
        partial.replace(path)
    config.hook.pytest_deselected(items=deselected)
    items[:] = selected
