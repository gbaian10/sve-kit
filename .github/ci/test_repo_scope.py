"""Exercise repository hook selection with real Git histories and the workflow shell."""

import os
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- runs Git and the workflow against synthetic repositories
import tempfile
import unittest
from pathlib import Path
from typing import override

import yaml

from hooks import ROOT

LIST_FILES_CONFIG = """\
repos:
  - repo: local
    hooks:
      - id: files
        name: files
        language: system
        entry: bash -c 'printf "CHANGED:%s\\n" "$@"' --
        verbose: true
"""


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
        self.write("docs/schema/build/build-db.md", "# Schema\n")
        self.write("dsl/engine-rules.schema.json", "{}\n")
        self.write(".markdownlint-cli2.yaml", "config: {}\n")
        self.base = self.commit()
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        self.script = next(
            step["run"]
            for step in workflow["jobs"]["repo"]["steps"]
            if str(step.get("name", "")).startswith("prek (")
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
        """Stub the hook runner and its selector; execute the workflow shell under errexit."""
        result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed Bash script with synthetic context
            [
                shutil.which("bash") or "bash",
                "-eu",
                "-o",
                "pipefail",
                "-c",
                "uv() { :; }\n"
                'prek() { printf "prek argv: %s\\nrange=%s\\n" "$*" "$GITLEAKS_RANGE"; }\n'
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
        return "\n".join(
            line for line in result.stdout.splitlines() if line.startswith("prek argv:")
        )

    def test_changed_files_and_public_push(self) -> None:
        """Ordinary PRs and pushes both use a range."""
        self.write("README.md", "# Updated\n")
        head = self.commit()
        assert f"--from-ref {self.base} --to-ref {head}" in self.scope(self.base, head)
        assert f"--from-ref {self.base} --to-ref {head}" in self.scope(
            self.base, head, event="push"
        )
        assert "--from-ref" in self.scope(head, head)

    def test_shared_inputs(self) -> None:
        """A settings-only PR must check existing files governed by those settings."""
        for name in (
            ".pre-commit-config.yaml",
            ".github/ci/hooks.toml",
            ".github/ci/hooks.py",
            "mise.toml",
            ".markdownlint-cli2.yaml",
            "docs/.markdownlint.json",
            ".editorconfig",
            ".gitattributes",
            ".gitleaks.toml",
            "bunfig.toml",
            "taplo.toml",
            ".taplo.toml",
            "carddb/uv.toml",
            "carddb/pyproject.toml",
            "pyproject.toml",
            "uv.lock",
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
                assert "--all-files" in self.scope(self.base, head, event="push")

    def test_deleted_or_renamed_inputs(self) -> None:
        """Prek drops removed inputs; retain aggregate checks for their consumers."""
        for name in ("docs/schema/build/build-db.md", ".markdownlint-cli2.yaml"):
            for rename in (False, True):
                with self.subTest(name=name, rename=rename):
                    self.command("reset", "--hard", self.base)
                    if rename:
                        self.command("mv", name, "moved.txt")
                    else:
                        self.command("rm", name)
                    head = self.commit()
                    for event in ("pull_request", "push"):
                        scope = self.scope(self.base, head, event=event)
                        assert (
                            "prek argv: run --config .pre-commit-config.yaml --all-files"
                            in scope
                        )
                        assert "--from-ref" not in scope

    def test_non_markdown_deletions_only_expand_the_link_check(self) -> None:
        """An unchanged Markdown file may link to a removed schema, image or script."""
        self.command("rm", "dsl/engine-rules.schema.json")
        head = self.commit()
        for event in ("pull_request", "push"):
            with self.subTest(event=event):
                scope = self.scope(self.base, head, event=event)
                assert scope.count("lychee --all-files") == 1
                assert f"--from-ref {self.base} --to-ref {head}" in scope

    def changed_files(self, base: str, head: str) -> list[str]:
        """Ask prek which paths it passes to a hook for this range."""
        prek = shutil.which("prek")
        if prek is None:
            raise RuntimeError(
                "prek is required on PATH; run mise install and activate mise"
            )
        # Keep the hook config outside the synthetic Git history.
        config = (
            Path(self.enterContext(tempfile.TemporaryDirectory())) / "list-files.yaml"
        )
        config.write_text(LIST_FILES_CONFIG)
        result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed executable, hook and commit arguments
            [
                prek,
                "run",
                "--config",
                str(config),
                "--from-ref",
                base,
                "--to-ref",
                head,
                "files",
            ],
            cwd=self.repo,
            env=self.git_env,
            check=True,
            capture_output=True,
            text=True,
        )
        return [
            line.lstrip().removeprefix("CHANGED:")
            for line in result.stdout.splitlines()
            if line.lstrip().startswith("CHANGED:")
        ]

    def test_diverged_base_and_rebased_head(self) -> None:
        """Compare against the merge base, including after rebasing past the event base."""
        self.command("checkout", "-qb", "target")
        self.write("target-only.md")
        target = self.commit()
        self.command("checkout", "-qb", "topic", self.base)
        self.write("topic.md")
        head = self.commit()
        assert self.changed_files(target, head) == ["topic.md"]
        assert self.changed_files(head, head) == []
        assert "--from-ref" in self.scope(target, head)
        self.command("rebase", "target")
        head = self.command("rev-parse", "HEAD")
        assert self.changed_files(target, head) == ["topic.md"]
        assert self.changed_files(self.base, head) == ["target-only.md", "topic.md"]
        assert "--from-ref" in self.scope(self.base, head)
