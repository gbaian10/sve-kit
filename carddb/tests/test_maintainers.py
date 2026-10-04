"""Configuration authority and alternate-reviewer integration use synthetic data."""

import sqlite3
import tomllib
from typing import TYPE_CHECKING

import pytest
from pydantic import TypeAdapter, ValidationError

from sve_carddb import maintainers
from sve_carddb.build_db.database import install_functions
from sve_carddb.build_db.t1 import compile_minimum
from sve_carddb.catalog.adoption_loader import load_adoptions
from sve_carddb.digital_links.loader import load_links
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, object_value

from . import adoption_fixtures, digital_link_fixtures

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path


@pytest.fixture
def configured(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Iterator[Path]:
    directory = tmp_path / "policy"
    directory.mkdir()
    file = directory / "maintainers.toml"
    file.write_text(
        'maintainers = ["gbaian10", "second-maintainer"]\n', encoding="utf-8"
    )
    monkeypatch.setattr(maintainers, "files", lambda _package: directory)
    maintainers.listed.cache_clear()
    try:
        yield file
    finally:
        maintainers.listed.cache_clear()


def test_shipped_policy_preserves_current_identity() -> None:
    assert maintainers.listed() == frozenset({"gbaian10"})


@pytest.mark.usefixtures("configured")
@pytest.mark.parametrize("reviewer", ["gbaian10", "second-maintainer"])
def test_shared_annotation_accepts_listed(
    reviewer: str,
) -> None:
    assert TypeAdapter(maintainers.Maintainer).validate_python(reviewer) == reviewer


@pytest.mark.usefixtures("configured")
@pytest.mark.parametrize(
    "reviewer",
    ["unlisted", "GBAIAN10", " gbaian10", "gbaian10 ", "", "Codex gpt-6.1-sol"],
)
def test_identity_is_exact_and_does_not_accept_tools(reviewer: str) -> None:
    assert not maintainers.is_maintainer(reviewer)
    with pytest.raises(ValidationError, match="repository-listed maintainer"):
        TypeAdapter(maintainers.Maintainer).validate_python(reviewer)


@pytest.mark.parametrize(
    "content",
    [
        "maintainers = []",
        "maintainers = [1]",
        "maintainers = [true]",
        'maintainers = [" gbaian10"]',
        'maintainers = ["bad\'account"]',
        'maintainers = ["same", "same"]',
        'maintainers = ["same", "SAME"]',
        'maintainers = ["gbaian10"]\nextra = true',
        'maintainers = "gbaian10"',
        "",
        "maintainers = [",
        'maintainers = ["-prefix"]',
    ],
)
def test_invalid_policy_fails_closed(configured: Path, content: str) -> None:
    configured.write_text(content, encoding="utf-8")
    with pytest.raises((ValidationError, tomllib.TOMLDecodeError)):
        maintainers.listed()


def test_missing_policy_has_no_account_fallback(configured: Path) -> None:
    configured.unlink()
    with pytest.raises(FileNotFoundError):
        maintainers.is_maintainer("gbaian10")


def test_environment_cannot_add_reviewers(
    configured: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    assert configured.is_file()
    monkeypatch.setenv("SVE_MAINTAINERS", '["unlisted"]')
    monkeypatch.setenv("SVE_MAINTAINERS_FILE", "/untrusted/policy.toml")
    assert maintainers.is_maintainer("second-maintainer")
    assert not maintainers.is_maintainer("unlisted")


@pytest.mark.usefixtures("configured")
def test_digital_link_loader_accepts_new_maintainer(tmp_path: Path) -> None:
    shard = digital_link_fixtures.envelope([digital_link_fixtures.record()])
    digital_link_fixtures.decision(shard)["reviewed_by"] = "second-maintainer"
    digital_link_fixtures.write(
        tmp_path, {"digital-links/links/synthetic/001.yaml": shard}
    )
    assert len(load_links(tmp_path).effective()) == 1


@pytest.mark.usefixtures("configured")
def test_adoption_loader_accepts_new_maintainer(tmp_path: Path) -> None:
    case = adoption_fixtures.make_case(tmp_path / "repository")
    for file in (case.root / "catalog-adoptions").rglob("*.yaml"):
        if file.name == "index.yaml":
            continue
        shard = object_value(read_yaml(file))
        object_value(array(shard["decisions"])[0])["reviewed_by"] = "second-maintainer"
        adoption_fixtures.write(
            case.root, file.relative_to(case.root).as_posix(), shard
        )
    adoption_fixtures.index(case.root)
    assert len(load_adoptions(case.root, entry="catalog-adoptions").effective()) == 6


@pytest.mark.usefixtures("configured")
@pytest.mark.parametrize(
    ("reviewer", "expected"),
    [
        ("gbaian10", 1),
        ("second-maintainer", 1),
        ("unlisted", 0),
        (" gbaian10", 0),
        (None, 0),
        (42, 0),
    ],
)
def test_sqlite_gate_uses_same_list(
    tmp_path: Path, reviewer: str | int | None, expected: int
) -> None:
    connection = sqlite3.connect(tmp_path / "build.sqlite")
    try:
        install_functions(connection, compile_minimum())
        assert connection.execute(
            "SELECT sve_is_maintainer(?)", (reviewer,)
        ).fetchone() == (expected,)
    finally:
        connection.close()
