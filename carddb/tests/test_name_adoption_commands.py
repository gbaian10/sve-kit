"""Trusted immutable adoption bases and conservative incomplete-history diagnostics."""

from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.cli import app
from sve_carddb.snapshot.values import object_value
from sve_carddb.translations.name_replay import _ancestor

from .adoption_fixtures import commit, git
from .name_replay_fixtures import Case, copied, make_case
from .test_name_replay import shards

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> Case:
    return make_case(tmp_path_factory.mktemp("name-adoption-cli-template"))


@pytest.fixture
def case(baseline: Case, tmp_path: Path) -> Case:
    return copied(baseline, tmp_path / "case")


@pytest.mark.parametrize("kind", ["assignment", "concept"])
def test_adoption_cli_checks_the_trusted_base_after_reader_replay(
    case: Case, kind: str
) -> None:
    base = git(case.frozen.root, "rev-parse", "HEAD")
    (case.frozen.root / "synthetic-feature.txt").write_text("Feature history.\n")
    feature = commit(case.frozen.root)
    record = case.assignment() if kind == "assignment" else case.concept()
    object_value(object_value(record["data"])["identity_basis"])[
        "authored_revision"
    ] = feature
    replay, inputs, _ = case.replay(shards(record))
    assert replay.uses
    args = [
        "translations",
        "check-name-adoption-base",
        "--authored",
        str(inputs.root),
        "--repository",
        str(inputs.repository),
        "--authored-revision",
        inputs.authored_revision,
        "--base-main-revision",
        base,
    ]
    result = CliRunner().invoke(app, args, env={"NO_COLOR": "1", "TERM": "dumb"})
    assert result.exit_code == 1
    assert isinstance(result.exception, ValueError)
    assert (
        str(result.exception) == "Name adoption basis is outside explicit base history"
    )
    # A trusted later base includes the same record background without changing the consumer.
    result = CliRunner().invoke(
        app, [*args[:-1], feature], env={"NO_COLOR": "1", "TERM": "dumb"}
    )
    assert result.exit_code == 0, result.exception
    assert result.output.strip() == (
        "Name adoption backgrounds belong to the explicit base-main history."
    )


def test_shallow_history_cannot_disprove_an_ancestor(tmp_path: Path) -> None:
    original = tmp_path / "original"
    original.mkdir()
    git(original, "init")
    (original / "synthetic.txt").write_text("First.\n")
    earlier = commit(original)
    (original / "synthetic.txt").write_text("Second.\n")
    later = commit(original)
    clone = tmp_path / "shallow"
    git(tmp_path, "clone", "--depth=1", original.as_uri(), str(clone))
    git(clone, "fetch", "--depth=1", original.as_uri(), earlier)
    assert git(clone, "rev-parse", "--is-shallow-repository") == "true"
    # Both immutable objects exist, but the later commit's parent edge is hidden.
    assert git(clone, "cat-file", "-t", earlier) == "commit"
    assert git(clone, "cat-file", "-t", later) == "commit"
    with pytest.raises(
        ValueError, match=r"^Name identity Git ancestry is unavailable$"
    ):
        _ancestor(PinnedRepository(clone), earlier, later)
    git(clone, "fetch", "--unshallow", original.as_uri())
    assert _ancestor(PinnedRepository(clone), earlier, later)
