# ruff: file-ignore[no-self-use, magic-value-comparison] -- unittest requires methods; compare the published coverage threshold literally
"""Check direct test ownership and the workflow's privacy/coverage contracts."""

import contextlib
import io
import os
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- executes the real workflow with synthetic tools
import tempfile
import tomllib
import unittest
from pathlib import Path
from typing import cast

import yaml

from hooks import ROOT, load_config, main, owners


class DirectTestTests(unittest.TestCase):
    """Direct steps must preserve the hooks' coverage gate without double execution."""

    def test_direct_hooks_skip_precommit(self) -> None:
        """Every direct hook is still owned, but omitted from its owner's hook run."""
        owned = owners()
        for hook_id, job in (("pytest", "python"), ("cargo-test", "rust")):
            assert owned[hook_id].job == job
            assert owned[hook_id].direct
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                assert main(["skip", job]) == 0
            assert hook_id in output.getvalue().strip().split(",")
        assert not owned["mypy"].direct
        assert main(["check"]) == 0

    def test_workflow_test_and_cleanup_contract(self) -> None:
        """Check exact commands and ensure failure summaries/cleanup still run."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        for job, command in (
            ("python", 'pytest -n "$PYTEST_WORKERS" --cov --durations=30'),
            (
                "rust",
                "cargo llvm-cov --locked --workspace -j 4 --summary-only --fail-under-lines 90",
            ),
            ("web", "bun run test --maxWorkers=4 --reporter=junit"),
        ):
            steps = cast("list[dict[str, object]]", jobs[job]["steps"])
            test = next(step for step in steps if step.get("id") == "tests")
            assert command in str(test["run"])
            assert "--output" in str(test["run"]) or "--junitxml" in str(test["run"])
            assert 'exit "$status"' in str(test["run"])
            summary = next(
                step for step in steps if "test_summary.py" in str(step.get("run"))
            )
            assert "!cancelled()" in str(summary["if"])
            cleanup = next(
                step
                for step in steps
                if str(step.get("name", "")).startswith("Remove private")
            )
            assert cleanup["if"] == "always()"
        assert not any(
            "upload-artifact@" in str(step.get("uses"))
            for job in jobs.values()
            for step in cast("list[dict[str, object]]", job.get("steps", []))
        )

    def test_runner_requires_private_same_origin_and_variable(self) -> None:
        """Every job defaults to hosted; self-hosted is restricted to trusted private work."""
        for name in ("ci.yml", "pr-title.yml"):
            workflow = cast(
                "dict[str, object]",
                yaml.safe_load((ROOT / ".github/workflows" / name).read_text()),
            )
            jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
            for job in jobs.values():
                runner = str(job["runs-on"])
                assert "github.event.repository.private &&" in runner
                assert (
                    "github.event.pull_request.head.repo.full_name == github.repository"
                    in runner
                )
                assert (
                    "vars.CI_RUNNER_LABELS && fromJSON(vars.CI_RUNNER_LABELS)" in runner
                )
                assert "|| 'ubuntu-latest'" in runner
                for step in cast("list[dict[str, object]]", job["steps"]):
                    if "actions/checkout@" in str(step.get("uses")):
                        assert cast("dict[str, object]", step["with"])["clean"] is True

    def test_python_coverage_contract_remains_combined_ninety(self) -> None:
        """Read the actual config so changing line-only or lowering the bar fails."""
        config = tomllib.loads((ROOT / "carddb/pyproject.toml").read_text())
        coverage = config["tool"]["coverage"]
        assert coverage["run"]["branch"] is True
        assert coverage["report"]["fail_under"] == 90
        config_hooks = load_config()
        hooks = cast("list[dict[str, object]]", config_hooks["repos"])
        pytest = next(
            hook
            for repo in hooks
            for hook in cast("list[dict[str, object]]", repo["hooks"])
            if hook["id"] == "pytest"
        )
        assert "--cov" in str(pytest["entry"])
        assert pytest["stages"] == ["manual"]

    def test_python_step_preserves_failure_without_printing_raw_log(self) -> None:
        """Execute the actual test step with a failing synthetic uv command."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        steps = cast("list[dict[str, object]]", jobs["python"]["steps"])
        script = str(next(step for step in steps if step.get("id") == "tests")["run"])
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            tool = root / "uv"
            tool.write_text(
                "#!/bin/bash\necho synthetic_private_diagnostic\nexit 7\n",
                encoding="utf-8",
            )
            tool.chmod(0o700)
            result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- exact workflow shell, fixed argv
                [shutil.which("bash") or "bash", "-e", "-o", "pipefail", "-c", script],
                env={
                    **os.environ,
                    "PATH": f"{root}{os.pathsep}{os.environ['PATH']}",
                    "REPORT_DIR": str(root / "reports"),
                    "SVE_DATA_DIR": str(root / "data"),
                    "PYTEST_WORKERS": "4",
                },
                capture_output=True,
                text=True,
                check=False,
            )
            assert result.returncode == 7
            assert "exit status: 7" in result.stdout
            assert "synthetic_private_diagnostic" not in result.stdout + result.stderr
            assert "synthetic_private_diagnostic" in (
                root / "reports/test.log"
            ).read_text(encoding="utf-8")
