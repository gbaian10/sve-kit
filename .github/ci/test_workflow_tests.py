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
TEST_JOBS = ("author-data", "python-tests", "rust-tests", "web")
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

    def test_component_inputs_select_their_jobs(self) -> None:
        """Check the workflow's include and exclude rules against real input paths."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        config = next(
            step["with"]
            for step in workflow["jobs"]["changes"]["steps"]
            if step.get("id") == "filter"
        )
        assert config["predicate-quantifier"] == "some-with-excludes"
        filters = yaml.safe_load(config["filters"])
        author = {"author_data", "rust"}
        python = {"python", "python_runtime"}
        cases = {
            "authored/rules/author-effects/BP01/000.yaml": author,
            "authored/rules/effect-bodies/BP06/000.yaml": author,
            "authored/rules/effects-index/BP01.yaml": author,
            "authored/rules/comprehensive/jp/1.29.0/ch01.json": {
                "python",
                "rust",
                "web",
            },
            "authored/translations/comprehensive-rules/zh-TW/1.29.0/ch01.json": {
                "python",
                "rust",
                "web",
            },
            "authored/rules/keyword-registry.yaml": {"python", "rust", "web"},
            "docs/sim/deck-construction-cases.json": {"web"},
            "sim/battle-host/src/lib.rs": {"rust", "web"},
            "sim/battle-contract/src/lib.rs": {"rust", "web"},
            "sim/engine/src/lib.rs": {"rust"},
            "tests/engine/card-input.json": {"rust"},
            "bunfig.toml": {"web"},
            "Cargo.lock": {"rust", "web"},
            "rust-toolchain.toml": {"rust", "web"},
            "uv.lock": python | {"card_data", "rust"},
            "carddb/src/sve_carddb/export/read_api.py": python
            | {"card_data", "rust", "scan"},
            "carddb/tests/support/repository_current.py": python | {"rust"},
            "carddb/tests/support/battle_projection.py": python | {"rust"},
            "carddb/tests/support/snapshot_project_fixtures.py": python | {"rust"},
            "carddb/tests/support/direct_current_fixture.py": python | {"rust"},
            "carddb/tests/support/direct_author_fixture.py": python | {"rust"},
            "carddb/tests/fixtures/author_entries/golden.yaml": python | {"rust"},
            "carddb/tests/domains/registry/test_registry.py": python | {"rust"},
            "carddb/tests/core/test_yaml_reader.py": python,
            "publish/pyproject.toml": python | {"card_data", "rust"},
            "publish/src/sve_publish/gc.py": python,
            "README.md": set(),
        }
        for path, expected in cases.items():
            with self.subTest(path=path):
                selected = set()
                for job, rules in filters.items():
                    # The only nested list comes from the shared Python patterns.
                    patterns = [
                        pattern
                        for rule in rules
                        for pattern in (rule if isinstance(rule, list) else [rule])
                    ]
                    matches = {
                        pattern
                        for pattern in patterns
                        if fnmatchcase(path, pattern.removeprefix("!"))
                    }
                    if matches and not any(
                        pattern.startswith("!") for pattern in matches
                    ):
                        selected.add(job)
                assert selected == expected

    def test_workflow_test_and_cleanup_contract(self) -> None:
        """Required component test and cleanup steps remain in their jobs."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        for job in (*TEST_JOBS, "python", *COVERAGE_JOBS):
            steps = cast("list[dict[str, object]]", jobs[job]["steps"])
            # A component job that runs can skip neither its tests nor its coverage gate.
            if job in TEST_JOBS:
                test = next(step for step in steps if step.get("id") == "tests")
                assert "run" in test
                assert "if" not in test
            if job in COVERAGE_JOBS:
                gate = next(step for step in steps if step.get("id") == "coverage")
                assert "if" not in gate
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

    def test_ci_does_not_require_private_checkout_or_credentials(self) -> None:
        """Python and Rust CI read public inputs without adding private credentials."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        for job in (
            "author-data",
            "python",
            "python-tests",
            "python-coverage",
            "rust",
            "rust-tests",
            "rust-coverage",
        ):
            steps = workflow["jobs"][job]["steps"]
            checkouts = [
                step
                for step in steps
                if "actions/checkout@" in str(step.get("uses", ""))
            ]
            assert len(checkouts) == 1
            assert all("ssh-key" not in step.get("with", {}) for step in checkouts)
            assert all("testdata.py" not in str(step.get("run", "")) for step in steps)

    def test_python_cleanup_removes_test_data_and_reports(self) -> None:
        """Python cleanup removes private reports and test data from a failed run."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        for job in ("author-data", "python", "python-tests"):
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

    def test_component_test_steps_preserve_failure_and_private_logs(self) -> None:
        """Fail each actual test step and ensure status and diagnostics stay private."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        for job, executable in (
            ("author-data", "uv"),
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
