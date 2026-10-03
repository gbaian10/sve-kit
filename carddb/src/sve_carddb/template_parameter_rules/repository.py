"""Inspect pinned Git trees and their history without following worktree paths."""

# ruff: file-ignore[suspicious-subprocess-import,subprocess-without-shell-equals-true] -- immutable Git reads use argument vectors, never a shell

import re
import subprocess
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_sources import PinnedRepository

DIRECTORY = "authored/template-parameter-rules"
PAIR = re.compile(
    r"^authored/template-parameter-rules/([a-z][a-z0-9_-]*)\.(policy|approval)\.yaml$"
)
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


def ancestor(repository: PinnedRepository, commit: str, main: str) -> None:
    """A branch-only matcher cannot serve as a durable main pin."""
    revision(repository, commit)
    revision(repository, main)
    result = subprocess.run(
        [
            repository.executable,
            "-C",
            str(repository.root),
            "merge-base",
            "--is-ancestor",
            commit,
            main,
        ],
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise ValueError(
            "Recognition matcher commit must be reachable from pinned main"
        )


def inventory(repository: PinnedRepository, commit: str) -> dict[str, str]:
    """Modes, filenames, sizes and whole pairs are validated before reading blob bytes."""
    revision(repository, commit)
    raw = git(repository, "ls-tree", "-r", "-l", "-z", commit, "--", "authored")
    result: dict[str, str] = {}
    groups: dict[str, set[str]] = {}
    for row in raw.split(b"\0"):
        if not row:
            continue
        header, separator, name = row.partition(b"\t")
        try:
            path = name.decode("utf-8", errors="strict")
            mode, kind, oid, size = header.split()
        except UnicodeError, ValueError:
            raise ValueError("Recognition Git tree has an invalid entry") from None
        if path not in {"authored", DIRECTORY} and not path.startswith(DIRECTORY + "/"):
            continue
        match = PAIR.fullmatch(path)
        if not separator or mode != b"100644" or kind != b"blob" or match is None:
            raise ValueError(
                "Recognition policy directory contains unsafe or extra entries"
            )
        if not size.isdigit() or int(size) >= LIMIT:
            raise ValueError("Recognition policy files must be smaller than one MiB")
        groups.setdefault(match[1], set()).add(match[2])
        result[path] = oid.decode("ascii")
    if any(types != {"policy", "approval"} for types in groups.values()):
        raise ValueError("Recognition policy directory must contain complete pairs")
    return result


def immutable(repository: PinnedRepository, commit: str) -> dict[str, str]:
    """Walk all ancestors so a modification followed by a revert also fails."""
    revision(repository, commit)
    if git(repository, "rev-parse", "--is-shallow-repository").strip() != b"false":
        raise ValueError(
            "Recognition immutable history requires a complete Git repository"
        )
    history = (
        git(
            repository,
            "rev-list",
            "--full-history",
            "--reverse",
            "--topo-order",
            "--parents",
            commit,
            "--",
            "authored",
        )
        .decode()
        .splitlines()
    )
    seen: dict[str, str] = {}
    trees: dict[str, dict[str, str]] = {}
    for row in history:
        ancestor_commit, *parents = row.split()
        tree = inventory(repository, ancestor_commit)
        for parent in parents:
            parent_tree = trees.get(parent)
            if parent_tree is None:
                parent_tree = inventory(repository, parent)
            if any(tree.get(path) != oid for path, oid in parent_tree.items()):
                raise ValueError(
                    "Recognition published policy pairs are immutable exact bytes"
                )
        if any(path in seen and seen[path] != oid for path, oid in tree.items()):
            raise ValueError(
                "Recognition published policy pairs are immutable exact bytes"
            )
        seen.update(tree)
        trees[ancestor_commit] = tree
    current = inventory(repository, commit)
    if current != seen:
        raise ValueError("Recognition published policy pairs are immutable exact bytes")
    return current
