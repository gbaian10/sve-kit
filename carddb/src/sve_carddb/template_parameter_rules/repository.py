"""Inspect current Git trees and explicit revisions without following worktree paths."""

# ruff: file-ignore[suspicious-subprocess-import,subprocess-without-shell-equals-true] -- current Git reads use argument vectors, never a shell

import re
import subprocess
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_sources import PinnedRepository

REVISION = re.compile(r"^[0-9a-f]{40}$")
LIMIT = 1048576


def git(repository: PinnedRepository, *args: str) -> bytes:
    """Missing immutable history is a refusal, never a reason to fetch."""
    result = subprocess.run(
        [repository.executable, "-C", str(repository.root), *args],
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise ValueError("Recognition immutable Git history is unavailable")
    return result.stdout


def revision(repository: PinnedRepository, value: str) -> None:
    """Resolve only a full commit, not a tag, short SHA or mutable branch name."""
    if REVISION.fullmatch(value) is None:
        raise ValueError("Recognition revision must be a full Git commit SHA")
    if git(repository, "cat-file", "-t", value).strip() != b"commit":
        raise ValueError("Recognition revision must identify a Git commit")
