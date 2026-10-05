"""Exercise repository hook selection with real Git histories and the workflow shell."""

import os
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- runs Git and the workflow against synthetic repositories
import sys
import tempfile
import unittest
from pathlib import Path
from typing import override

import yaml

from hooks import ROOT


class RepoScopeTests(unittest.TestCase):
    """Shared settings and removed build inputs must not escape repository checks."""

    @override
    def setUp(self) -> None:
        """Prepare a clean history without invoking the developer's Git hooks."""
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.repo = Path(temp.name)
        self.git_env = {
            **{
                key: value
                for key, value in os.environ.items()
                if not key.startswith("GIT_")
            },
            "GIT_CONFIG_GLOBAL": "/dev/null",
            "GIT_CONFIG_NOSYSTEM": "1",
        }
        self.git = shutil.which("git") or "git"
        self.command("init", "-q")
        self.command("config", "user.name", "Scope Test")
        self.command("config", "user.email", "scope@example.invalid")
        self.write("README.md", "# Baseline\n")
        self.write("docs/schema/build-db.md", "# Schema\n")
        self.write(".markdownlint-cli2.yaml", "config: {}\n")
        self.base = self.commit()
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        self.script = next(
            step["run"]
            for step in workflow["jobs"]["repo"]["steps"]
            if str(step.get("name", "")).startswith("pre-commit (")
        )

    def command(self, *args: str) -> str:
        """Use real Git so merge bases, deletions and pathspecs behave as in CI."""
        return subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed executable and argv, isolated repo
            [self.git, "-c", "core.hooksPath=/dev/null", *args],
            cwd=self.repo,
            env=self.git_env,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()

    def write(self, name: str, contents: str = "synthetic input\n") -> None:
        """Create an input in the isolated repository."""
        path = self.repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents)

    def commit(self) -> str:
        """Record the current synthetic tree."""
        self.command("add", "--all")
        self.command("-c", "commit.gpgsign=false", "commit", "-qm", "synthetic")
        return self.command("rev-parse", "HEAD")

    def scope(self, base: str, head: str, *, event: str = "pull_request") -> str:
        """Stub only uv; execute the unchanged workflow selection under Bash errexit."""
        result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed Bash script with synthetic context
            [
                shutil.which("bash") or "bash",
                "-eu",
                "-o",
                "pipefail",
                "-c",
                'uv() { if [[ "$*" == *"pre-commit run"* ]]; then '
                'printf "pre-commit argv: %s\\nrange=%s\\n" "$*" "$GITLEAKS_RANGE"; fi; };\n'
                + self.script,
            ],
            cwd=self.repo,
            env={
                **self.git_env,
                "BASE": base,
                "HEAD": head,
                "GITHUB_EVENT_NAME": event,
            },
            capture_output=True,
            text=True,
            check=True,
        )
        assert f"range={base}..{head}" in result.stdout
        return next(
            line
            for line in result.stdout.splitlines()
            if line.startswith("pre-commit argv:")
        )

    def test_changed_files_and_public_push(self) -> None:
        """Ordinary PRs use a range; public pushes retain the complete scan."""
        self.write("README.md", "# Updated\n")
        head = self.commit()
        assert f"--from-ref {self.base} --to-ref {head}" in self.scope(self.base, head)
        assert "--all-files" in self.scope(self.base, head, event="push")
        assert "--from-ref" in self.scope(head, head)

    def test_shared_inputs(self) -> None:
        """A settings-only PR must check existing files governed by those settings."""
        for name in (
            ".pre-commit-config.yaml",
            ".github/ci/hooks.toml",
            ".github/ci/hooks.py",
            ".markdownlint-cli2.yaml",
            "docs/.markdownlint.json",
            ".editorconfig",
            ".gitattributes",
            ".gitleaks.toml",
            "taplo.toml",
            ".taplo.toml",
            "carddb/uv.toml",
            "carddb/pyproject.toml",
            "carddb/uv.lock",
            ".github/actionlint.yaml",
            ".github/zizmor.yml",
            ".github/actions/example/action.yml",
            ".github/workflows/reusable.yml",
        ):
            with self.subTest(name=name):
                self.command("reset", "--hard", self.base)
                self.write(name)
                head = self.commit()
                assert "--all-files" in self.scope(self.base, head)

    def test_deleted_or_renamed_inputs(self) -> None:
        """Pre-commit drops removed inputs; retain aggregate checks for their consumers."""
        for name in ("docs/schema/build-db.md", ".markdownlint-cli2.yaml"):
            for rename in (False, True):
                with self.subTest(name=name, rename=rename):
                    self.command("reset", "--hard", self.base)
                    if rename:
                        self.command("mv", name, "moved.txt")
                    else:
                        self.command("rm", name)
                    head = self.commit()
                    assert "--all-files" in self.scope(self.base, head)

    def changed_files(self, base: str, head: str) -> list[str]:
        """Query the installed pre-commit implementation in the synthetic checkout."""
        result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed interpreter, source and commit arguments
            [
                sys.executable,
                "-c",
                (
                    "import sys; from pre_commit.git import get_changed_files; "
                    "print('\\n'.join(get_changed_files(*sys.argv[1:])))"
                ),
                base,
                head,
            ],
            cwd=self.repo,
            env=self.git_env,
            check=True,
            capture_output=True,
            text=True,
        )
        return result.stdout.splitlines()

    def test_diverged_base_and_rebased_head(self) -> None:
        """Compare against the merge base, including after rebasing past the event base."""
        self.command("checkout", "-qb", "target")
        self.write("target-only.md")
        target = self.commit()
        self.command("checkout", "-qb", "topic", self.base)
        self.write("topic.md")
        head = self.commit()
        assert self.changed_files(target, head) == ["topic.md"]
        assert "--from-ref" in self.scope(target, head)
        self.command("rebase", "target")
        head = self.command("rev-parse", "HEAD")
        assert self.changed_files(target, head) == ["topic.md"]
        assert self.changed_files(self.base, head) == ["target-only.md", "topic.md"]
        assert "--from-ref" in self.scope(self.base, head)
