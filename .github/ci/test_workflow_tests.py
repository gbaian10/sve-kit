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
                'python3 .github/ci/testdata.py rust-tests "$REPORT_DIR/coverage.json"',
            ),
            ("web", "bun run test --maxWorkers=4 --reporter=junit"),
        ):
            steps = cast("list[dict[str, object]]", jobs[job]["steps"])
            test = next(step for step in steps if step.get("id") == "tests")
            assert command in str(test["run"])
            assert (
                "coverage.json" in str(test["run"])
                if job == "rust"
                else "--output" in str(test["run"]) or "--junitxml" in str(test["run"])
            )
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

    def test_every_job_uses_fixed_github_hosted_runner(self) -> None:
        """Every workflow job stays hosted even if a repository runner variable is reintroduced."""
        for name in ("ci.yml", "pr-title.yml"):
            source = (ROOT / ".github/workflows" / name).read_text()
            workflow = cast(
                "dict[str, object]",
                yaml.safe_load(source),
            )
            assert "self-hosted" not in source
            assert "CI_RUNNER" not in source
            assert "cache-local-path" not in source
            jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
            for job in jobs.values():
                assert job["runs-on"] == "ubuntu-latest"
                for step in cast("list[dict[str, object]]", job["steps"]):
                    if "actions/checkout@" in str(step.get("uses")):
                        assert cast("dict[str, object]", step["with"])["clean"] is True

    def test_private_checkout_and_key_are_only_required_for_full_scope(self) -> None:
        """Forks omit checkout rather than attempting it without credentials."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        for component in ("python", "rust"):
            steps = workflow["jobs"][component]["steps"]
            select = next(step for step in steps if step.get("id") == "pin")
            assert (
                select["run"] == f"python3 .github/ci/testdata.py configure {component}"
            )
            checkout = next(
                step
                for step in steps
                if step.get("with", {}).get("path") == ".testdata"
            )
            key = next(
                step
                for step in steps
                if step.get("name") == "Require the private-data deploy key"
            )
            verify = next(
                step
                for step in steps
                if step.get("name") == "Verify required private test data"
            )
            for step in (checkout, key, verify):
                assert step["if"] == "steps.pin.outputs.mode == 'required'"
            assert checkout["with"]["ref"] == "${{ steps.pin.outputs.commit }}"
            assert checkout["with"]["persist-credentials"] is False
            assert verify["run"] == f"python3 .github/ci/testdata.py verify {component}"
            assert (
                steps.index(select)
                < steps.index(key)
                < steps.index(checkout)
                < steps.index(verify)
            )
            for value, expected in (("", 1), ("synthetic-key", 0)):
                result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- real fixed key guard, synthetic secret
                    [shutil.which("bash") or "bash", "-eu", "-c", key["run"]],
                    env={**os.environ, "DEPLOY_KEY": value},
                    capture_output=True,
                    check=False,
                )
                assert result.returncode == expected
                assert not value or value not in result.stdout.decode()
        test = next(
            step
            for step in workflow["jobs"]["python"]["steps"]
            if step.get("id") == "tests"
        )
        assert '--cov-fail-under="$SVE_CI_COVERAGE_THRESHOLD"' in test["run"]
        assert test["env"]["TMPDIR"] == "${{ runner.temp }}"
        cleanup = next(
            step
            for step in workflow["jobs"]["python"]["steps"]
            if step.get("name") == "Remove private Python reports and test data"
        )
        assert cleanup["if"] == "always()"
        assert '"${RUNNER_TEMP}"/pytest-of-*' in cleanup["run"]

    def test_only_pr_runs_are_cancelled(self) -> None:
        """Main cache writers must finish even when another commit is pushed."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        concurrency = cast("dict[str, object]", workflow["concurrency"])
        assert (
            concurrency["group"]
            == "${{ github.workflow }}-${{ github.event.pull_request.number || github.run_id }}"
        )
        assert (
            concurrency["cancel-in-progress"]
            == "${{ github.event_name == 'pull_request' }}"
        )

    def test_private_main_only_starts_ci_ok_and_missing_flag_runs_full_path(
        self,
    ) -> None:
        """Evaluate job guards, including absent privacy data, before any runner starts."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        for event, private, expected in (
            ("pull_request", "true", set(jobs)),
            ("pull_request", "false", set(jobs)),
            ("push", "true", {"ci-ok"}),
            ("push", "false", set(jobs)),
            ("push", "", set(jobs)),
            ("pull_request", "", set(jobs)),
        ):
            selected = {"ci-ok"}
            for name, job in jobs.items():
                if name == "ci-ok":
                    assert job["if"] == "always()"
                    continue
                expression = str(job["if"]).removeprefix("${{ ").removesuffix(" }}")
                expression = (
                    expression.replace("github.event.repository.private", '"$PRIVATE"')
                    .replace("github.event_name", '"$EVENT_NAME"')
                    .replace("github.ref", '"$REF"')
                    .replace("!(", "! (")
                )
                for component in ("python", "rust", "web"):
                    expression = expression.replace(
                        f"needs.changes.outputs.{component}", "'true'"
                    )
                result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- real guards with fixed synthetic context
                    [shutil.which("bash") or "bash", "-c", f"[[ {expression} ]]"],
                    env={
                        **os.environ,
                        "EVENT_NAME": event,
                        "PRIVATE": private,
                        "REF": "refs/heads/main",
                    },
                    capture_output=True,
                    check=False,
                )
                assert result.returncode in {0, 1}
                if result.returncode == 0:
                    selected.add(name)
            with self.subTest(event=event, private=private):
                assert selected == expected
        ci_steps = cast("list[dict[str, object]]", jobs["ci-ok"]["steps"])
        assert ci_steps[0]["if"] == "env.MAIN_MAINTENANCE != 'true'"
        maintenance = next(
            step
            for step in ci_steps
            if step.get("uses") == "./.github/actions/main-maintenance"
        )
        assert maintenance["if"] == "env.MAIN_MAINTENANCE == 'true'"
        mode = cast("dict[str, str]", jobs["ci-ok"]["env"])["MAIN_MAINTENANCE"]
        assert (
            mode
            == "${{ github.event_name == 'push' && github.ref == 'refs/heads/main' && github.event.repository.private == true }}"
        )
        for component in ("python", "rust", "web"):
            test = next(
                step
                for step in cast("list[dict[str, object]]", jobs[component]["steps"])
                if step.get("id") == "tests"
            )
            assert "if" not in test

    def test_cache_keys_targets_and_main_writers_are_preserved(self) -> None:
        """Main maintenance keeps cache writers and preserves the existing Rust prefix."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        for job in jobs.values():
            for step in cast("list[dict[str, object]]", job["steps"]):
                action = str(step.get("uses", ""))
                inputs = cast("dict[str, object]", step.get("with", {}))
                if "Swatinem/rust-cache@" in action:
                    assert "prefix-key" not in inputs
                    assert "cache-targets" not in inputs
                    assert (
                        inputs["save-if"]
                        == "${{ github.event_name == 'push' && github.ref == 'refs/heads/main' }}"
                    )
                if "astral-sh/setup-uv@" in action:
                    assert inputs["enable-cache"] is True
                    assert (
                        inputs["save-cache"]
                        == "${{ github.event_name == 'push' && github.ref == 'refs/heads/main' }}"
                    )
                    assert inputs["cache-suffix"] == "${{ github.job }}"
                if "actions/cache/save@" in action:
                    assert str(step["if"]).startswith(
                        "github.event_name == 'push' && github.ref == 'refs/heads/main'"
                    )
                if "jdx/mise-action@" in action and inputs.get("cache") is True:
                    assert (
                        inputs["cache_save"]
                        == "${{ github.event_name == 'push' && github.ref == 'refs/heads/main' }}"
                    )
        web_steps = cast("list[dict[str, object]]", jobs["web"]["steps"])
        install = next(
            step
            for step in web_steps
            if step.get("run") == "bun install --frozen-lockfile"
        )
        assert "if" not in install

    def test_rust_cleanup_retains_target_before_cache_post_save(self) -> None:
        """Execute cleanup in a sandbox: discard private inputs, keep compiled dependencies."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        steps = cast("list[dict[str, object]]", jobs["rust"]["steps"])
        script = str(
            next(
                step
                for step in steps
                if str(step.get("name", "")).startswith("Remove private")
            )["run"]
        )
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for name in ("target", ".testdata", "reports"):
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
            assert not (root / ".testdata").exists()
            assert not (root / "reports").exists()

    def test_rust_cache_warming_compiles_only_and_preserves_failure(self) -> None:
        """The real cache step uses coverage flags, never executes tests, and fails closed."""
        action = cast(
            "dict[str, object]",
            yaml.safe_load(
                (ROOT / ".github/actions/main-maintenance/action.yml").read_text()
            ),
        )
        steps = cast(
            "list[dict[str, object]]",
            cast("dict[str, object]", action["runs"])["steps"],
        )
        step = next(step for step in steps if step.get("id") == "rust-compile")
        assert step["if"] == "steps.rust-cache.outputs.cache-hit != 'true'"
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            tool = root / "cargo"
            tool.write_text(
                '#!/bin/bash\nif [[ "$*" == "llvm-cov show-env --sh" ]]; then\n'
                '  [[ "$FAKE_STATUS" == env-fail ]] && exit 7\n'
                '  echo "export CARGO_LLVM_COV=1"\n'
                'elif [[ "$*" == "test --locked --workspace -j 4 --no-run" ]]; then\n'
                '  [[ "$FAKE_STATUS" == build-fail ]] && exit 8\n'
                "else\n"
                '  [[ "$CARGO_LLVM_COV" == 1 && "$*" == "test --locked --workspace -j 4 --target-dir $GITHUB_WORKSPACE/target/llvm-cov-target --no-run" ]] || exit 9\n'
                '  [[ "$FAKE_STATUS" == build-fail ]] && exit 8\n'
                "fi\nexit 0\n",
                encoding="utf-8",
            )
            tool.chmod(0o700)
            for status, expected in (("ok", 0), ("env-fail", 7), ("build-fail", 8)):
                result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- workflow shell with synthetic cargo
                    [
                        shutil.which("bash") or "bash",
                        "-eu",
                        "-o",
                        "pipefail",
                        "-c",
                        str(step["run"]),
                    ],
                    env={
                        **os.environ,
                        "PATH": f"{root}{os.pathsep}{os.environ['PATH']}",
                        "FAKE_STATUS": status,
                        "GITHUB_WORKSPACE": str(root),
                    },
                    capture_output=True,
                    check=False,
                )
                assert result.returncode == expected

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
