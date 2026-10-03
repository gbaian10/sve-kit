"""Immutable Git byte history and complete index closures do not trust a worktree."""

import copy
import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.template_translations.files import immutable, read
from sve_carddb.template_translations.loader import load_templates

from .adoption_fixtures import commit
from .template_intake_fixtures import (
    DEFINITIONS,
    INVENTORY,
    intake_case,
    policy_git,
    recognition_term_source,
    write,
)

if TYPE_CHECKING:
    from pathlib import Path

    from .template_intake_fixtures import Case

__all__ = ("intake_case", "policy_git", "recognition_term_source")


def exact(message: str) -> str:
    """Refusals are anchored to one complete message."""
    return "^" + re.escape(message) + "$"


@pytest.mark.parametrize("action", ["change", "remove", "change_then_revert"])
def test_published_bytes_are_immutable_even_after_revert(
    intake_case: Case, tmp_path: Path, action: str
) -> None:
    root = intake_case.fork(tmp_path / "published", published=True)
    path = root / "authored" / DEFINITIONS
    before = path.read_bytes()
    if action == "remove":
        path.unlink()
        index_path = root / "authored/translations/index.yaml"
        index = object_value(parse(index_path.read_bytes()))
        object_value(index["includes"]).pop(DEFINITIONS)
        index_path.write_bytes(canonical(index))
    else:
        path.write_bytes(before + b"\n")
    revision = commit(root)
    if action == "change_then_revert":
        path.write_bytes(before)
        revision = commit(root)
    with pytest.raises(
        ValueError,
        match=exact(
            "Template published shards or inventories were modified or removed"
        ),
    ):
        immutable(PinnedRepository(root), revision)


@pytest.mark.parametrize(
    ("action", "message"),
    [
        ("missing_index", "Template translation index must explicitly exist"),
        ("extra_shard", "Template indexed file closure differs from Git"),
        ("missing_shard", "Template indexed file closure differs from Git"),
        ("symlink", "Template Git closure contains unsafe or unsupported files"),
        (
            "directory_symlink",
            "Template Git closure contains unsafe or unsupported files",
        ),
        (
            "unsupported_file",
            "Template Git closure contains unsafe or unsupported files",
        ),
        ("execute_mode", "Template Git closure contains unsafe or unsupported files"),
        ("oversize", "Template Git closure contains unsafe or unsupported files"),
        ("hash", "Template indexed input hash mismatch"),
        ("wrong_area", "Template indexed path is unsafe or in the wrong area"),
        ("overlap", "Template shard and inventory paths must be disjoint"),
        ("sequence", "Template indexed sequence has a gap or duplicate"),
        ("boolean_format", "Invalid template translation index"),
    ],
)
def test_exact_file_closure_refusals(  # ruff: ignore[complex-structure,too-many-branches] -- independent malformed Git trees each reach their exact boundary guard
    intake_case: Case, tmp_path: Path, action: str, message: str
) -> None:
    root = intake_case.fork(tmp_path / "bad")
    revision = write(root, copy.deepcopy(intake_case.files))
    index_path = root / "authored/translations/index.yaml"
    index = object_value(parse(index_path.read_bytes()))
    includes = object_value(index["includes"])
    inventories = object_value(index["inventories"])
    file = root / "authored" / DEFINITIONS
    if action == "missing_index":
        index_path.unlink()
    elif action == "extra_shard":
        file.with_name("002.yaml").write_bytes(file.read_bytes())
    elif action == "missing_shard":
        file.unlink()
    elif action == "symlink":
        file.unlink()
        file.symlink_to("../../../../outside")
    elif action == "directory_symlink":
        file.unlink()
        file.parent.rmdir()
        file.parent.symlink_to("../../../outside", target_is_directory=True)
    elif action == "unsupported_file":
        file.with_name("extra.txt").write_text("Synthetic")
    elif action == "execute_mode":
        file.chmod(0o755)
    elif action == "oversize":
        file.write_bytes(b"x" * 1048576)
    elif action == "hash":
        includes[DEFINITIONS] = "sha256:" + "d" * 64
    elif action == "wrong_area":
        includes[INVENTORY] = inventories.pop(INVENTORY)
    elif action == "overlap":
        includes[INVENTORY] = inventories[INVENTORY]
    elif action == "sequence":
        next_file = file.with_name("003.yaml")
        file.rename(next_file)
        includes[next_file.relative_to(root / "authored").as_posix()] = includes.pop(
            DEFINITIONS
        )
    else:
        index["translation_authored_format"] = True
    if action != "missing_index":
        index_path.write_bytes(canonical(index))
    revision = commit(root)
    with pytest.raises(ValueError, match=exact(message)):
        read(PinnedRepository(root), revision)


@pytest.mark.parametrize("value", ["a" * 7, "HEAD", "a" * 40])
def test_missing_or_mutable_git_revision_is_rejected(
    intake_case: Case, value: str
) -> None:
    message = (
        "Recognition revision must be a full Git commit SHA"
        if len(value) != 40
        else "Recognition immutable Git history is unavailable"
    )
    with pytest.raises(ValueError, match=exact(message)):
        read(PinnedRepository(intake_case.repository), value)


def test_checkout_edits_do_not_change_the_pinned_snapshot(
    intake_case: Case, tmp_path: Path
) -> None:
    root = intake_case.fork(tmp_path / "dirty", published=True)
    (root / "authored" / DEFINITIONS).write_text("Invalid uncommitted bytes")
    snapshot = load_templates(
        PinnedRepository(root), intake_case.revision, intake_case.sources()
    )
    assert len(snapshot.records()) == 4
    assert (
        read(PinnedRepository(root), intake_case.revision).index
        == intake_case.verified.index
    )


def test_index_cannot_repoint_an_old_frozen_entry(
    intake_case: Case, tmp_path: Path
) -> None:
    root = intake_case.fork(tmp_path / "repoint", published=True)
    path = root / "authored/translations/index.yaml"
    value = object_value(parse(path.read_bytes()))
    object_value(value["inventories"])[INVENTORY] = digest(b"Different canonical input")
    path.write_bytes(canonical(value))
    revision = commit(root)
    with pytest.raises(ValueError, match=exact("Template indexed input hash mismatch")):
        immutable(PinnedRepository(root), revision)


def test_shallow_clone_cannot_validate_immutable_history(
    intake_case: Case, tmp_path: Path
) -> None:
    from .adoption_fixtures import git  # ruff: ignore[import-outside-top-level] -- real depth-limited local clone, with isolated fixture Git settings

    root = tmp_path / "shallow"
    git(
        intake_case.repository,
        "clone",
        "--depth=1",
        intake_case.repository.as_uri(),
        str(root),
    )
    with pytest.raises(
        ValueError,
        match=exact("Template immutable replay requires complete Git history"),
    ):
        immutable(PinnedRepository(root), intake_case.revision)
