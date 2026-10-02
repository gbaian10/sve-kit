"""Batch Git framing remains a strict boundary for immutable wording inputs."""

import re
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- synthesize fixed Git process results at its checked boundary
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_sources import PinnedRepository

from .wording_adoption_fixtures import commit, git

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture(scope="module")
def immutable(tmp_path_factory: pytest.TempPathFactory) -> tuple[Path, str]:
    root = tmp_path_factory.mktemp("immutable-wording-git")
    (root / "synthetic").mkdir()
    (root / "synthetic/a").write_bytes(b"Synthetic\n\x00exact\n")
    (root / "synthetic/b").write_bytes(b"\n")
    git(root, "init")
    return root, commit(root)


def test_batch_reads_exact_blobs_and_caches_only_complete_responses(
    immutable: tuple[Path, str], monkeypatch: pytest.MonkeyPatch
) -> None:
    root, revision = immutable
    repository = PinnedRepository(root)
    result = repository.read_many(revision, ("synthetic/a", "synthetic/b"))
    assert result == {"synthetic/a": b"Synthetic\n\x00exact\n", "synthetic/b": b"\n"}
    assert repository.read_many(revision, ()) == {}
    monkeypatch.setattr(
        "sve_carddb.catalog.adoption_sources.subprocess.run",
        lambda *_args, **_kwargs: pytest.fail("Cache should avoid another Git process"),
    )
    assert repository.read_many(revision, ("synthetic/a",)) == {
        "synthetic/a": result["synthetic/a"]
    }


@pytest.mark.parametrize(
    "name",
    [
        "missing",
        "synthetic",
        "/absolute",
        "../outside",
        "synthetic/../a",
        "synthetic//a",
        "synthetic/a\nHEAD",
        "synthetic/a\x00",
    ],
)
def test_missing_non_blob_and_unsafe_paths_fail_without_partial_cache(
    immutable: tuple[Path, str], name: str
) -> None:
    root, revision = immutable
    repository = PinnedRepository(root)
    message = (
        "Immutable dependency batch contains a missing/non-blob object"
        if name in {"missing", "synthetic"}
        else "Unsafe batch dependency path"
    )
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        repository.read_many(revision, ("synthetic/a", name))
    assert not repository.cache


@pytest.mark.parametrize(
    "output",
    [
        b"bad\n",
        b"oid blob nope\n",
        b"oid blob 99\nx\n",
        b"oid blob 1\nx!",
        b"oid blob 1\nx\nextra",
    ],
    ids=["header", "size", "truncated", "terminator", "extra"],
)
def test_git_response_framing_cannot_inject_a_dependency(
    immutable: tuple[Path, str], monkeypatch: pytest.MonkeyPatch, output: bytes
) -> None:
    root, revision = immutable
    repository = PinnedRepository(root)
    monkeypatch.setattr(
        "sve_carddb.catalog.adoption_sources.subprocess.run",
        lambda *_args, **_kwargs: subprocess.CompletedProcess([], 0, output, b""),
    )
    message = {
        b"bad\n": "Immutable dependency batch contains a missing/non-blob object",
        b"oid blob nope\n": "Immutable dependency batch contains a missing/non-blob object",
        b"oid blob 99\nx\n": "Invalid immutable Git batch framing",
        b"oid blob 1\nx!": "Invalid immutable Git batch framing",
        b"oid blob 1\nx\nextra": "Unexpected immutable Git batch output",
    }[output]
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        repository.read_many(revision, ("synthetic/a",))
    assert not repository.cache


def test_batch_requires_a_full_immutable_revision(immutable: tuple[Path, str]) -> None:
    root, _ = immutable
    with pytest.raises(ValueError, match="full Git SHA"):
        PinnedRepository(root).read_many("HEAD", ("synthetic/a",))
