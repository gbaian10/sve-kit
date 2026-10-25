from __future__ import annotations

import subprocess  # ruff: ignore[suspicious-subprocess-import] -- monkeypatch Git lookup in unit test
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- pytest fixture annotation
from types import SimpleNamespace

import pytest  # ruff: ignore[typing-only-third-party-import] -- pytest fixture annotation

from sve_carddb import docker_mounts
from sve_carddb.workflows.offline import Inputs


def test_recipe_mounts_include_git_common_dir_and_preserve_same_path(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo = tmp_path / "checkout"
    archive = tmp_path / "archive"
    common = tmp_path / "common.git"
    for path in (repo, archive, common):
        path.mkdir()
    recipe_path = tmp_path / "recipe.json"
    recipe_path.write_text("{}")
    recipe = SimpleNamespace(repo=repo, protected_inputs=lambda: (repo, archive))
    monkeypatch.setattr(Inputs, "model_validate_json", lambda _raw: recipe)
    monkeypatch.setattr(
        subprocess,
        "check_output",
        lambda *_args, **_kwargs: f"{common}\n",
    )

    roots = docker_mounts.readonly_roots(recipe_path)

    assert roots == (repo, archive, common)
    override = docker_mounts.compose_override(roots)
    for root in roots:
        assert f'source: "{root}"' in override
        assert f'target: "{root}"' in override
    assert override.count("read_only: true") == 3
