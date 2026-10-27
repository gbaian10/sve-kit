# ruff: file-ignore[no-self-use] -- unittest requires methods
"""Check essential workflow behavior, cleanup and privacy contracts."""

import contextlib
import io
import os
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- executes the real workflow with synthetic tools
import tempfile
import unittest
from fnmatch import fnmatchcase
from pathlib import Path
from typing import cast

import yaml

from hooks import ROOT, main, owners


class DirectTestTests(unittest.TestCase):
    """Direct steps must preserve the hooks' coverage gate without double execution."""

    def test_direct_hooks_skip_precommit(self) -> None:
        """Every direct hook is still owned, but omitted from its owner's hook run."""
        owned = owners()
        for hook_id, job in (
            ("pytest", "python"),
            ("pytest-publish", "python"),
            ("cargo-test", "rust"),
        ):
            assert owned[hook_id].job == job
            assert owned[hook_id].direct
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                assert main(["skip", job]) == 0
            assert hook_id in output.getvalue().strip().split(",")
        assert not owned["mypy"].direct
        assert main(["check"]) == 0

    def test_bun_settings_run_the_web_job(self) -> None:
        """A bunfig.toml-only PR must rerun the web format check, the one with Tailwind class order."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        filters = yaml.safe_load(
            next(
                step["with"]["filters"]
                for step in workflow["jobs"]["changes"]["steps"]
                if step.get("id") == "filter"
            )
        )
        assert any(fnmatchcase("bunfig.toml", pattern) for pattern in filters["web"])

    def test_publish_changes_select_the_python_job(self) -> None:
        """Publisher changes select the Python job that runs publisher tests."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        filters = yaml.safe_load(
            next(
                step["with"]["filters"]
                for step in workflow["jobs"]["changes"]["steps"]
                if step.get("id") == "filter"
            )
        )
        for path in (
            "carddb/src/sve_carddb/export/read_api.py",
            "publish/src/sve_publish/gc.py",
        ):
            assert any(fnmatchcase(path, pattern) for pattern in filters["python"])
        steps = workflow["jobs"]["python"]["steps"]
        assert any(step.get("id") == "publish-tests" for step in steps)

    def test_workflow_test_and_cleanup_contract(self) -> None:
        """Required component test and cleanup steps remain in their jobs."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        for job in ("python", "rust", "web"):
            steps = cast("list[dict[str, object]]", jobs[job]["steps"])
            test = next(step for step in steps if step.get("id") == "tests")
            assert "run" in test
            assert "if" not in test
            cleanup = next(
                step for step in steps if str(step.get("name", "")).startswith("Remove")
            )
            assert "run" in cleanup
        assert not any(
            "upload-artifact@" in str(step.get("uses"))
            for job in jobs.values()
            for step in cast("list[dict[str, object]]", job.get("steps", []))
        )

    def test_python_does_not_require_private_checkout_or_credentials(self) -> None:
        """Python CI reads public inputs without adding private credentials."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        steps = workflow["jobs"]["python"]["steps"]
        checkouts = [
            step for step in steps if "actions/checkout@" in str(step.get("uses", ""))
        ]
        assert len(checkouts) == 1
        assert all("ssh-key" not in step.get("with", {}) for step in checkouts)
        assert all("testdata.py" not in str(step.get("run", "")) for step in steps)

    def test_component_test_steps_are_not_conditionally_skipped(self) -> None:
        """A component job that runs cannot skip its required test step."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        for component in ("python", "rust", "web"):
            test = next(
                step
                for step in cast("list[dict[str, object]]", jobs[component]["steps"])
                if step.get("id") == "tests"
            )
            assert "if" not in test

    def test_rust_cleanup_retains_target_before_cache_post_save(self) -> None:
        """Execute cleanup in a sandbox: discard raw reports, keep compiled dependencies."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        steps = cast("list[dict[str, object]]", jobs["rust"]["steps"])
        script = str(
            next(
                step for step in steps if str(step.get("name", "")).startswith("Remove")
            )["run"]
        )
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for name in ("target", "reports"):
                (root / name).mkdir()
                (root / name / "marker").touch()
            result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- sandboxed real cleanup, fixed argv
                [shutil.which("bash") or "bash", "-eu", "-c", script],
                cwd=root,
                env={**os.environ, "REPORT_DIR": str(root / "reports")},
                capture_output=True,
                check=False,
            )
            assert result.returncode == 0
            assert (root / "target/marker").exists()
            assert not (root / "reports").exists()

    def test_python_cleanup_removes_test_data_and_reports(self) -> None:
        """Python cleanup removes private reports and test data from a failed run."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        steps = workflow["jobs"]["python"]["steps"]
        script = str(
            next(
                step["run"]
                for step in steps
                if step.get("name") == "Remove Python reports and test data"
            )
        )
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for path in (
                root / "reports/test.log",
                root / "data/private-input",
                root / "sve-publish-reports/report.json",
                root / "pytest-of-test/marker",
                root / "carddb/htmlcov/marker",
            ):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.touch()
            result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- sandboxed real cleanup, fixed argv
                [shutil.which("bash") or "bash", "-eu", "-c", script],
                cwd=root,
                env={
                    **os.environ,
                    "REPORT_DIR": str(root / "reports"),
                    "SVE_DATA_DIR": str(root / "data"),
                    "RUNNER_TEMP": str(root),
                },
                capture_output=True,
                check=False,
            )
            assert result.returncode == 0
            assert not (root / "reports").exists()
            assert not (root / "data").exists()
            assert not (root / "sve-publish-reports").exists()
            assert not (root / "pytest-of-test").exists()
            assert not (root / "carddb/htmlcov").exists()

    def test_web_cleanup_removes_private_reports(self) -> None:
        """Web cleanup removes the raw test report after the step has run."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        steps = workflow["jobs"]["web"]["steps"]
        script = str(
            next(
                step["run"]
                for step in steps
                if step.get("name") == "Remove private Web reports"
            )
        )
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            report = root / "reports/test.log"
            report.parent.mkdir(parents=True)
            report.touch()
            result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- sandboxed real cleanup, fixed argv
                [shutil.which("bash") or "bash", "-eu", "-c", script],
                cwd=root,
                env={**os.environ, "REPORT_DIR": str(root / "reports")},
                capture_output=True,
                check=False,
            )
            assert result.returncode == 0
            assert not report.parent.exists()

    def test_rust_changes_select_the_rust_job(self) -> None:
        """Rust source and public engine inputs select Rust validation."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        filters = yaml.safe_load(
            next(
                step["with"]["filters"]
                for step in workflow["jobs"]["changes"]["steps"]
                if step.get("id") == "filter"
            )
        )
        for path in (
            "sim/engine/src/lib.rs",
            "tests/engine/card-input.json",
        ):
            assert any(fnmatchcase(path, pattern) for pattern in filters["rust"])

    def test_rust_does_not_require_private_checkout_or_credentials(self) -> None:
        """Rust CI uses the public checkout without private credentials."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        steps = workflow["jobs"]["rust"]["steps"]
        checkouts = [
            step for step in steps if "actions/checkout@" in str(step.get("uses", ""))
        ]
        assert len(checkouts) == 1
        assert all("ssh-key" not in step.get("with", {}) for step in checkouts)

    def test_component_test_steps_preserve_failure_and_private_logs(self) -> None:
        """Fail each actual test step and ensure status and diagnostics stay private."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        for job, executable in (("python", "uv"), ("rust", "python3"), ("web", "bun")):
            with self.subTest(job=job), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                steps = cast("list[dict[str, object]]", jobs[job]["steps"])
                script = str(
                    next(step for step in steps if step.get("id") == "tests")["run"]
                )
                failure_status = 7
                tool = root / executable
                tool.write_text(
                    f"#!/bin/bash\necho synthetic_private_diagnostic\nexit {failure_status}\n",
                    encoding="utf-8",
                )
                tool.chmod(0o700)
                result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- execute the workflow step with a failing synthetic tool
                    [
                        shutil.which("bash") or "bash",
                        "-e",
                        "-o",
                        "pipefail",
                        "-c",
                        script,
                    ],
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
                assert result.returncode == failure_status
                assert (
                    "synthetic_private_diagnostic" not in result.stdout + result.stderr
                )
                log = next((root / "reports").glob("*.log"))
                assert "synthetic_private_diagnostic" in log.read_text(encoding="utf-8")
