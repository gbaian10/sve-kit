from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from sve_carddb.domains.construction.adoption import Settings as Construction
from sve_carddb.domains.construction.adoption_reports import StorePin
from sve_carddb.workflows import docker_mounts
from sve_carddb.workflows.offline import Inputs, RegionalInput

from ..support.adoption_fixtures import git


def clone(root: Path) -> Path:
    root.mkdir()
    git(root, "init", "-q")
    git(
        root,
        "-c",
        "user.name=Synthetic",
        "-c",
        "user.email=synthetic@example.invalid",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "init",
    )
    return root


def recipe(
    repo: Path, archive: Path, construction: Construction | None = None
) -> Inputs:
    pins = tuple(
        RegionalInput(
            region=region,
            card_batch="sha256:" + "a" * 64,
            image_batch="sha256:" + "b" * 64,
            parser_version="synthetic-v1",
        )
        for region in ("en", "jp")
    )
    return Inputs(
        repo=repo,
        archive=archive,
        store_id="test-store",
        sources=pins,
        revision="a" * 40,
        as_of="2026-10-02",
        data_version="preview-20261002T010203Z-0001",
        published_at="2026-10-02T00:00:00Z",
        feedback_url="https://example.invalid/feedback",
        grammar_version="synthetic-v1",
        normalizer_version="synthetic-v1",
        construction=construction,
    )


def test_clone_mounts_repo_archive_and_its_own_git_dir(tmp_path: Path) -> None:
    repo = clone(tmp_path / "checkout")
    archive = tmp_path / "archive"
    archive.mkdir()

    assert docker_mounts.readonly_roots(recipe(repo, archive)) == (
        repo,
        archive,
        repo / ".git",
    )


def test_worktree_mounts_the_main_checkouts_git_dir(tmp_path: Path) -> None:
    main = clone(tmp_path / "main")
    worktree = tmp_path / "worktree"
    git(main, "worktree", "add", "-q", "--detach", str(worktree))
    archive = tmp_path / "archive"
    archive.mkdir()

    roots = docker_mounts.readonly_roots(recipe(worktree, archive))

    assert roots == (worktree, archive, main / ".git")


def test_construction_stores_are_mounted_once_each(tmp_path: Path) -> None:
    repo = clone(tmp_path / "checkout")
    archive = tmp_path / "archive"
    source, root, backup, restored = (
        tmp_path / name for name in ("source", "review", "backup", "restored")
    )
    for path in (archive, source, root, backup, restored):
        path.mkdir()
    pin = StorePin(
        store_id="review",
        root=root,
        backup=backup,
        restored=restored,
        inventory_sha256="sha256:" + "c" * 64,
    )
    construction = Construction(
        # The archive doubles as a source store, as in a single-store layout.
        source_stores={"a": source, "b": archive},
        review_stores=(pin,),
    )

    roots = docker_mounts.readonly_roots(recipe(repo, archive, construction))

    assert roots == (repo, archive, source, root, backup, restored, repo / ".git")


@pytest.mark.parametrize("relative", [False, True])
def test_non_canonical_roots_are_rejected(tmp_path: Path, *, relative: bool) -> None:
    repo = clone(tmp_path / "checkout")
    real = tmp_path / "real"
    real.mkdir()
    link = tmp_path / "link"
    link.symlink_to(real)
    archive = Path("real") if relative else link

    with pytest.raises(ValueError, match="absolute and symlink-free"):
        docker_mounts.readonly_roots(recipe(repo, archive))


def test_symlinked_repo_is_rejected_before_git_runs(tmp_path: Path) -> None:
    real = clone(tmp_path / "checkout")
    link = tmp_path / "link"
    link.symlink_to(real)
    archive = tmp_path / "archive"
    archive.mkdir()

    with pytest.raises(ValueError, match="absolute and symlink-free"):
        docker_mounts.readonly_roots(recipe(link, archive))


def test_override_binds_every_root_read_only_at_the_same_path(tmp_path: Path) -> None:
    roots = (tmp_path / "a", tmp_path / 'quote"d: [x]')

    override = yaml.safe_load(docker_mounts.compose_override(roots))

    assert override == {
        "services": {
            "export": {
                "volumes": [
                    {
                        "type": "bind",
                        "source": str(root),
                        "target": str(root),
                        "read_only": True,
                    }
                    for root in roots
                ]
            }
        }
    }
