"""Synthetic commits ignore developer hooks while retaining real immutable Git objects."""

import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- inspect only the synthetic Git object created by the fixture
from typing import TYPE_CHECKING

import pytest

from .product_identity_fixtures import commit

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize("configuration", ["global", "environment"])
def test_synthetic_commit_ignores_external_hooks(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, configuration: str
) -> None:
    hooks = tmp_path / "hooks"
    hooks.mkdir()
    hook = hooks / "pre-commit"
    hook.write_text("#!/bin/sh\nexit 87\n")
    hook.chmod(0o700)
    if configuration == "global":
        config = tmp_path / "external.gitconfig"
        config.write_text(f"[core]\n\thooksPath = {hooks.as_posix()}\n")
        monkeypatch.setenv("GIT_CONFIG_GLOBAL", str(config))
    else:
        monkeypatch.setenv("GIT_CONFIG_COUNT", "1")
        monkeypatch.setenv("GIT_CONFIG_KEY_0", "core.hooksPath")
        monkeypatch.setenv("GIT_CONFIG_VALUE_0", str(hooks))
    root = tmp_path / "checkout/authored"
    root.mkdir(parents=True)
    content = b"synthetic: immutable Git bytes\n"
    (root / "example.yaml").write_bytes(content)
    revision = commit(root)
    executable = shutil.which("git")
    assert executable is not None
    stored = subprocess.check_output(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed arguments read the isolated immutable object
        [
            executable,
            "-C",
            str(root.parent),
            "show",
            revision + ":authored/example.yaml",
        ]
    )
    assert stored == content
    assert len(revision) == 40
    (root / "example.yaml").write_bytes(b"changed")
    assert (
        subprocess.check_output(  # ruff: ignore[subprocess-without-shell-equals-true] -- the same object must still retain its original bytes
            [
                executable,
                "-C",
                str(root.parent),
                "show",
                revision + ":authored/example.yaml",
            ]
        )
        == content
    )
