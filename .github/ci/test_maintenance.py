# ruff: file-ignore[no-self-use, magic-value-comparison] -- unittest requires methods; three original hook-cache readers must be retained
"""Exercise cache-only preparation and its real final gate without downloading tools."""

import json
import os
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- runs checked-in shell with synthetic results
import tempfile
import unittest
from pathlib import Path
from typing import ClassVar, cast, override

import yaml

from hooks import ROOT, install_config, load_config, owners


class MaintenanceTests(unittest.TestCase):
    """Cache maintenance must retain keys and never turn incomplete work into a green check."""

    steps: ClassVar[list[dict[str, object]]]
    gate: ClassVar[str]

    @classmethod
    @override
    def setUpClass(cls) -> None:
        """Load the immutable action definition once for these synthetic cases."""
        action = cast(
            "dict[str, object]",
            yaml.safe_load(
                (ROOT / ".github/actions/main-maintenance/action.yml").read_text()
            ),
        )
        cls.steps = cast(
            "list[dict[str, object]]",
            cast("dict[str, object]", action["runs"])["steps"],
        )
        cls.gate = str(
            next(
                step
                for step in cls.steps
                if step.get("name") == "Cache maintenance gate"
            )["run"]
        )

    def run_gate(self, results: dict[str, object], *, status: str = "success") -> int:
        """Run only the real final shell, with a disposable summary file."""
        with tempfile.TemporaryDirectory() as folder:
            result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed gate argv and synthetic state
                [
                    shutil.which("bash") or "bash",
                    "-eu",
                    "-o",
                    "pipefail",
                    "-c",
                    self.gate,
                ],
                env={
                    **os.environ,
                    "STEPS": json.dumps(results),
                    "JOB_STATUS": status,
                    "GITHUB_STEP_SUMMARY": str(Path(folder) / "summary"),
                },
                capture_output=True,
                check=False,
            )
        return result.returncode

    @staticmethod
    def success(*, cache_hit: bool) -> dict[str, object]:
        """Minimal completed phases, with compilation required only on a miss."""
        return {
            **{
                name: {"outcome": "success"}
                for name in (
                    "python-deps",
                    "repo-hooks",
                    "python-hooks",
                    "rust-hooks",
                    "mypy-cache",
                    "web-deps",
                )
            },
            "rust-cache": {
                "outcome": "success",
                "outputs": {"cache-hit": "true" if cache_hit else "false"},
            },
            "rust-compile": {"outcome": "skipped" if cache_hit else "success"},
        }

    def test_warm_and_cold_completed_maintenance(self) -> None:
        """Warm skips compilation; cold must finish compilation before passing."""
        assert self.run_gate(self.success(cache_hit=True)) == 0
        assert self.run_gate(self.success(cache_hit=False)) == 0

    def test_missing_failed_skipped_and_cancelled_phases_fail(self) -> None:
        """Every required maintenance phase must actually complete."""
        for name in (
            "python-deps",
            "repo-hooks",
            "python-hooks",
            "rust-hooks",
            "mypy-cache",
            "rust-cache",
            "web-deps",
        ):
            for outcome in (None, "failure", "skipped", "cancelled"):
                with self.subTest(name=name, outcome=outcome):
                    results = self.success(cache_hit=True)
                    if outcome is None:
                        del results[name]
                    else:
                        results[name] = {"outcome": outcome}
                    assert self.run_gate(results) != 0

    def test_compile_missing_or_skipped_on_cache_miss_fails(self) -> None:
        """An absent cache cannot excuse missing coverage compilation."""
        results = self.success(cache_hit=False)
        results["rust-compile"] = {"outcome": "skipped"}
        assert self.run_gate(results) != 0
        del results["rust-compile"]
        assert self.run_gate(results) != 0

    def test_other_failures_and_job_cancellation_fail(self) -> None:
        """Optional cache/tool steps cannot hide an actual action failure."""
        results = self.success(cache_hit=True)
        results["cache-save"] = {"outcome": "failure"}
        assert self.run_gate(results) != 0
        assert self.run_gate(self.success(cache_hit=True), status="cancelled") != 0

    def test_install_configs_keep_environment_definitions_and_ownership(self) -> None:
        """Prepare only existing CI hook environments; never include tests or mutation tools."""
        config, owned = load_config(), owners()
        configs = {
            job: install_config(config, owned, job)
            for job in ("repo", "python", "rust")
        }
        for job, selected in configs.items():
            assert selected.get("default_language_version") == config.get(
                "default_language_version"
            )
            for repo in cast("list[dict[str, object]]", selected["repos"]):
                for hook in cast("list[dict[str, object]]", repo["hooks"]):
                    name = str(hook.get("alias", hook["id"]))
                    assert owned[name].job == job
                    assert not owned[name].direct
        repo_text = yaml.safe_dump(configs["repo"])
        python_text = yaml.safe_dump(configs["python"])
        rust_text = yaml.safe_dump(configs["rust"])
        assert "gitleaks-range" in repo_text
        assert "gitleaks-staged" not in repo_text
        assert "ruff-format" in python_text
        assert "pytest" not in python_text
        assert "cargo-test" not in rust_text
        assert "cargo-mutants" not in rust_text
        assert "markdownlint-cli2" not in python_text

    def test_preparation_does_not_execute_lint_types_or_tests(self) -> None:
        """The only code execution is locked dependency installation and no-run compilation."""
        commands = [str(step["run"]) for step in self.steps if "run" in step]
        assert "uv sync --directory carddb --locked" in commands
        assert "bun install --frozen-lockfile" in commands
        assert not any(
            "pre-commit run" in command
            or "mypy " in command
            or "pytest " in command
            or "bun run" in command
            or "cargo clippy" in command
            for command in commands
        )
        installs = [
            command for command in commands if "pre-commit install-hooks" in command
        ]
        assert len(installs) == 3
        for command in installs:
            assert "install-config" in command

    def test_cache_keys_match_the_unmodified_component_readers(self) -> None:
        """The ci-ok writer must not strand caches under its own job name or changed paths."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        uv_inputs = [
            cast("dict[str, object]", step["with"])
            for step in self.steps
            if "astral-sh/setup-uv@" in str(step.get("uses"))
        ]
        assert {inputs["cache-suffix"] for inputs in uv_inputs} == {
            "repo",
            "python",
            "rust",
            "commit",
        }
        assert [
            inputs["cache-suffix"]
            for inputs in uv_inputs
            if inputs.get("restore-cache", True)
        ] == ["python"]
        for job in ("repo", "python", "rust"):
            full_steps = cast("list[dict[str, object]]", jobs[job]["steps"])
            original = next(
                step for step in full_steps if step.get("id") == "hook-cache"
            )
            writer = next(
                step for step in self.steps if step.get("id") == f"{job}-hook-cache"
            )
            expected = dict(cast("dict[str, object]", original["with"]))
            for key in ("key", "restore-keys"):
                expected[key] = str(expected[key]).replace("${{ github.job }}", job)
            assert writer["with"] == expected
        original_mypy = next(
            step
            for step in cast("list[dict[str, object]]", jobs["python"]["steps"])
            if step.get("id") == "mypy-cache"
        )
        maintained_mypy = next(
            step for step in self.steps if step.get("id") == "mypy-cache"
        )
        assert maintained_mypy["with"] == original_mypy["with"]
        rust = next(step for step in self.steps if step.get("id") == "rust-cache")
        inputs = cast("dict[str, object]", rust["with"])
        assert inputs["shared-key"] == "rust"
        assert "cache-targets" not in inputs
        assert "prefix-key" not in inputs
        for step in self.steps:
            if "actions/cache/save@" in str(step.get("uses")):
                assert str(step["if"]).startswith(
                    "github.event_name == 'push' && github.ref == 'refs/heads/main'"
                )
