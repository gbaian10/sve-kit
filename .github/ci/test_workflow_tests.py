# ruff: file-ignore[no-self-use] -- unittest requires methods
"""Check essential workflow behavior, cleanup and privacy contracts."""

import contextlib
import io
import json
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

# Jobs that run a component's tests, and the jobs that gate the shards' merged coverage.
TEST_JOBS = ("python-tests", "rust-tests", "web")
COVERAGE_JOBS = ("python-coverage", "rust-coverage")
COVERAGE_DIRS = {
    "python-tests": "sve-python-coverage",
    "rust-tests": "sve-rust-coverage",
}


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
        for job in (*TEST_JOBS, "python", *COVERAGE_JOBS):
            steps = cast("list[dict[str, object]]", jobs[job]["steps"])
            if job in TEST_JOBS:
                test = next(step for step in steps if step.get("id") == "tests")
                assert "run" in test
                assert "if" not in test
            cleanup = next(
                step for step in steps if str(step.get("name", "")).startswith("Remove")
            )
            assert "run" in cleanup
            assert cleanup["if"] == "always()"
        uploads = [
            (name, step)
            for name, job in jobs.items()
            for step in cast("list[dict[str, object]]", job.get("steps", []))
            if "upload-artifact@" in str(step.get("uses"))
        ]
        # Only the shards' coverage data crosses jobs; raw reports stay on the runner.
        assert sorted(name for name, _ in uploads) == ["python-tests", "rust-tests"]
        for name, step in uploads:
            options = cast("dict[str, object]", step["with"])
            assert options["path"] == f"${{{{ runner.temp }}}}/{COVERAGE_DIRS[name]}/"
            assert options["retention-days"] == 1
            assert step["if"] == "${{ steps.tests.outcome == 'success' }}"

    def test_python_does_not_require_private_checkout_or_credentials(self) -> None:
        """Python CI reads public inputs without adding private credentials."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        for job in ("python", "python-tests", "python-coverage"):
            steps = workflow["jobs"][job]["steps"]
            checkouts = [
                step
                for step in steps
                if "actions/checkout@" in str(step.get("uses", ""))
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
        for component in TEST_JOBS:
            test = next(
                step
                for step in cast("list[dict[str, object]]", jobs[component]["steps"])
                if step.get("id") == "tests"
            )
            assert "if" not in test
        for component in COVERAGE_JOBS:
            gate = next(
                step
                for step in cast("list[dict[str, object]]", jobs[component]["steps"])
                if step.get("id") == "coverage"
            )
            assert "if" not in gate

    def test_rust_cleanup_retains_target_before_cache_post_save(self) -> None:
        """Execute cleanup in a sandbox: discard raw reports, keep compiled dependencies."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        steps = cast("list[dict[str, object]]", jobs["rust-tests"]["steps"])
        script = str(
            next(
                step for step in steps if str(step.get("name", "")).startswith("Remove")
            )["run"]
        )
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for name in ("target", "reports", "coverage"):
                (root / name).mkdir()
                (root / name / "marker").touch()
            result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- sandboxed real cleanup, fixed argv
                [shutil.which("bash") or "bash", "-eu", "-c", script],
                cwd=root,
                env={
                    **os.environ,
                    "REPORT_DIR": str(root / "reports"),
                    "COVERAGE_DIR": str(root / "coverage"),
                },
                capture_output=True,
                check=False,
            )
            assert result.returncode == 0
            assert (root / "target/marker").exists()
            assert not (root / "reports").exists()
            assert not (root / "coverage").exists()

    def test_python_cleanup_removes_test_data_and_reports(self) -> None:
        """Python cleanup removes private reports and test data from a failed run."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        for job in ("python", "python-tests"):
            steps = workflow["jobs"][job]["steps"]
            script = str(
                next(
                    step["run"]
                    for step in steps
                    if step.get("name") == "Remove Python reports and test data"
                )
            )
            with self.subTest(job=job), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                for path in (
                    root / "reports/test.log",
                    root / "coverage/shard-1.coverage",
                    root / "data/private-input",
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
                        "COVERAGE_DIR": str(root / "coverage"),
                        "SVE_DATA_DIR": str(root / "data"),
                        "RUNNER_TEMP": str(root),
                    },
                    capture_output=True,
                    check=False,
                )
                assert result.returncode == 0
                assert not (root / "reports").exists()
                assert not (root / "data").exists()
                assert not (root / "pytest-of-test").exists()
                assert not (root / "carddb/htmlcov").exists()
                if job == "python-tests":
                    assert not (root / "coverage").exists()

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
        for job in ("rust", "rust-tests", "rust-coverage"):
            steps = workflow["jobs"][job]["steps"]
            checkouts = [
                step
                for step in steps
                if "actions/checkout@" in str(step.get("uses", ""))
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
        for job, executable in (
            ("python-tests", "uv"),
            ("rust-tests", "python3"),
            ("web", "bun"),
        ):
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
                        "COVERAGE_DIR": str(root / "coverage"),
                        "SHARD": "1/3",
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
                # A failed shard hands nothing to the coverage job.
                assert not (root / "coverage").exists()


class ShardTests(unittest.TestCase):
    """Sharding keeps the full test scope and the unchanged 90% gates."""

    def workflow(self) -> dict[str, dict[str, object]]:
        """Return the CI jobs."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        return cast("dict[str, dict[str, object]]", workflow["jobs"])

    def test_every_shard_job_reaches_its_coverage_gate_and_ci_ok(self) -> None:
        """Shards are 1..N, feed one coverage job, and every job counts for its component."""
        jobs = self.workflow()
        for tests, coverage, component in (
            ("python-tests", "python-coverage", "python"),
            ("rust-tests", "rust-coverage", "rust"),
        ):
            strategy = cast("dict[str, object]", jobs[tests]["strategy"])
            shards = cast("dict[str, list[int]]", strategy["matrix"])["shard"]
            assert shards == list(range(1, len(shards) + 1))
            assert strategy["fail-fast"] is False
            assert jobs[coverage]["needs"] == ["changes", tests]
            for job in (tests, coverage):
                assert jobs[job]["if"] == (
                    f"${{{{ needs.changes.outputs.{component} == 'true' }}}}"
                )

    def test_ci_ok_rejects_failed_cancelled_or_missing_required_shards(self) -> None:
        """Run the real gate: every new required job must succeed; unchanged components may skip."""
        jobs = self.workflow()
        script = str(cast("list[dict[str, object]]", jobs["ci-ok"]["steps"])[0]["run"])
        for component in ("python", "rust"):
            for state in ("success", "failure", "cancelled", "skipped"):
                for job in (f"{component}-tests", f"{component}-coverage"):
                    with self.subTest(job=job, state=state):
                        needs: dict[str, dict[str, object]] = {
                            name: {"result": state if name == job else "success"}
                            for name in cast("list[str]", jobs["ci-ok"]["needs"])
                        }
                        needs["changes"]["outputs"] = {
                            "python": "true",
                            "rust": "true",
                        }
                        result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- actual gate with synthetic job results
                            [shutil.which("bash") or "bash", "-eu", "-c", script],
                            env={**os.environ, "NEEDS": json.dumps(needs)},
                            capture_output=True,
                            check=False,
                        )
                        assert (result.returncode == 0) == (state == "success")
            needs = {
                name: {"result": "skipped" if name.startswith(component) else "success"}
                for name in cast("list[str]", jobs["ci-ok"]["needs"])
            }
            needs["changes"]["outputs"] = {component: "false"}
            result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- actual gate with an unchanged component
                [shutil.which("bash") or "bash", "-eu", "-c", script],
                env={**os.environ, "NEEDS": json.dumps(needs)},
                capture_output=True,
                check=False,
            )
            assert result.returncode == 0

    def test_coverage_steps_apply_gates_and_preserve_failure(self) -> None:
        """The real shell steps invoke the coverage gates and propagate their failure."""
        jobs = self.workflow()
        for job, executable in (
            ("python-coverage", "uv"),
            ("rust-coverage", "python3"),
        ):
            with self.subTest(job=job), tempfile.TemporaryDirectory() as folder:
                failure_status = 7
                root = Path(folder)
                script = str(
                    next(
                        step["run"]
                        for step in cast("list[dict[str, object]]", jobs[job]["steps"])
                        if step.get("id") == "coverage"
                    )
                )
                tool = root / "bin" / executable
                tool.parent.mkdir()
                tool.write_text(
                    '#!/bin/bash\nprintf "%s\\n" "$@" >> "$ARGV_FILE"\n'
                    'for argument in "$@"; do\n'
                    f'  if [[ "$argument" == report || "$argument" == *.py ]]; then exit {failure_status}; fi\n'
                    "done\n",
                    encoding="utf-8",
                )
                tool.chmod(0o700)
                result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- actual coverage step with a failing external tool
                    [shutil.which("bash") or "bash", "-eu", "-c", script],
                    cwd=root,
                    env={
                        **os.environ,
                        "PATH": f"{tool.parent}{os.pathsep}{os.environ['PATH']}",
                        "DATA_DIR": str(root / "coverage"),
                        "ARGV_FILE": str(root / "argv"),
                    },
                    capture_output=True,
                    check=False,
                )
                assert result.returncode == failure_status
                arguments = (root / "argv").read_text().splitlines()
                if job == "python-coverage":
                    assert "combine" in arguments
                    assert "--fail-under=90" in arguments
                else:
                    assert arguments == [
                        ".github/ci/rust_coverage.py",
                        str(root / "coverage"),
                    ]

    def test_successful_shards_hand_over_coverage_only(self) -> None:
        """Run each real test step with a passing fake tool: only coverage data and counts are kept."""
        jobs = self.workflow()
        for job, executable, outputs in (
            (
                "python-tests",
                "uv",
                "mkdir -p carddb/htmlcov\necho data > carddb/htmlcov/.coverage",
            ),
            (
                "rust-tests",
                "python3",
                'mkdir -p "$REPORT_DIR/coverage"\necho data > "$REPORT_DIR/coverage/shard.profdata"',
            ),
        ):
            with self.subTest(job=job), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                script = str(
                    next(
                        step
                        for step in cast("list[dict[str, object]]", jobs[job]["steps"])
                        if step.get("id") == "tests"
                    )["run"]
                )
                tool = root / "bin" / executable
                tool.parent.mkdir()
                tool.write_text(
                    f'#!/bin/bash\nprintf "%s\\n" "$@" > "$ARGV_FILE"\necho "$PRIVATE"\n{outputs}\n'
                    'echo \'{"selected": 1}\' > "$REPORT_DIR/shard.json"\n',
                    encoding="utf-8",
                )
                tool.chmod(0o700)
                result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- execute the workflow step with a passing synthetic tool
                    [shutil.which("bash") or "bash", "-e", "-c", script],
                    cwd=root,
                    env={
                        **os.environ,
                        "PATH": f"{tool.parent}{os.pathsep}{os.environ['PATH']}",
                        "REPORT_DIR": str(root / "reports"),
                        "COVERAGE_DIR": str(root / "coverage"),
                        "SVE_DATA_DIR": str(root / "data"),
                        "SHARD": "2/3",
                        "PYTEST_WORKERS": "4",
                        "PRIVATE": "synthetic_private_diagnostic",
                        "ARGV_FILE": str(root / "argv"),
                    },
                    capture_output=True,
                    text=True,
                    check=False,
                )
                assert result.returncode == 0, result.stderr
                arguments = (root / "argv").read_text().splitlines()
                assert "--shard=2/3" in arguments
                if job == "python-tests":
                    assert "--cov-fail-under=0" in arguments
                kept = sorted(
                    path.relative_to(root / "coverage").as_posix()
                    for path in (root / "coverage").rglob("*")
                    if path.is_file()
                )
                assert kept == (
                    ["shard-2.coverage", "shard-2.json"]
                    if job == "python-tests"
                    else ["shard-2.json", "shard-2/shard.profdata"]
                )
                assert "synthetic_private_diagnostic" in (
                    root / "reports/test.log"
                ).read_text(encoding="utf-8")
                assert not any(
                    "synthetic_private_diagnostic" in path.read_text()
                    for path in (root / "coverage").rglob("*")
                    if path.is_file()
                )
