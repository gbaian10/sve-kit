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
from fnmatch import fnmatchcase
from pathlib import Path
from typing import cast

import yaml

from hooks import ROOT, load_config, main, owners


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
        assert "bunfig.toml" in filters["web"]

    def test_publish_is_in_the_python_job_with_independent_coverage(self) -> None:
        """Reader changes and fork PRs must retain the complete publisher tests."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        filters = yaml.safe_load(
            next(
                step["with"]["filters"]
                for step in workflow["jobs"]["changes"]["steps"]
                if step.get("id") == "filter"
            )
        )
        assert "carddb/**" in filters["python"]
        assert "publish/**" in filters["python"]
        steps = workflow["jobs"]["python"]["steps"]
        tests = next(step for step in steps if step.get("id") == "publish-tests")
        assert tests["if"] == "${{ !cancelled() }}"
        assert "--locked pytest" in tests["run"]
        assert "--cov-fail-under" not in tests["run"]
        config = tomllib.loads((ROOT / "publish/pyproject.toml").read_text())
        assert config["tool"]["coverage"]["run"]["branch"] is True
        assert config["tool"]["coverage"]["report"]["fail_under"] == 92
        summary = next(
            step for step in steps if step.get("name") == "Publish test summary"
        )
        assert "steps.publish-tests.outcome" in summary["if"]
        assert 'test_summary.py publish "$REPORT_DIR"' in summary["run"]

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
                "python3 .github/ci/rust_tests.py llvm-cov --locked --workspace --summary-only",
            ),
            ("web", "bun run test:all --maxWorkers=4 --reporter=junit"),
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
                step for step in steps if str(step.get("name", "")).startswith("Remove")
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

    def test_python_runs_full_scope_without_private_checkout(self) -> None:
        """Every Python PR uses the same gate and needs no private credentials."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        steps = workflow["jobs"]["python"]["steps"]
        checkouts = [
            step for step in steps if "actions/checkout@" in str(step.get("uses", ""))
        ]
        assert len(checkouts) == 1
        assert all("ssh-key" not in step.get("with", {}) for step in checkouts)
        assert all("testdata.py" not in str(step.get("run", "")) for step in steps)
        test = next(
            step
            for step in workflow["jobs"]["python"]["steps"]
            if step.get("id") == "tests"
        )
        assert "--cov-fail-under=90" in test["run"]
        assert test["env"]["TMPDIR"] == "${{ runner.temp }}"
        cleanup = next(
            step
            for step in workflow["jobs"]["python"]["steps"]
            if step.get("name") == "Remove Python reports and test data"
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

    def test_jobs_run_on_every_event_and_tests_are_unconditional(self) -> None:
        """Job guards use change outputs; selected components always execute their test step."""
        workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        jobs = cast("dict[str, dict[str, object]]", workflow["jobs"])
        assert jobs["ci-ok"]["if"] == "always()"
        for name in ("changes", "repo", "commit"):
            assert "if" not in jobs[name], name
        for component in ("python", "rust", "web"):
            assert (
                jobs[component]["if"]
                == "${{ needs.changes.outputs." + component + " == 'true' }}"
            )
            test = next(
                step
                for step in cast("list[dict[str, object]]", jobs[component]["steps"])
                if step.get("id") == "tests"
            )
            assert "if" not in test

    def test_cache_keys_targets_and_main_writers_are_preserved(self) -> None:
        """Only main writes caches, and the Rust cache keeps its default prefix."""
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
        python_steps = cast("list[dict[str, object]]", jobs["python"]["steps"])
        restore = next(step for step in python_steps if step.get("id") == "mypy-cache")
        save = next(
            step
            for step in python_steps
            if "steps.mypy-cache.outputs.cache-primary-key" in str(step.get("with", {}))
        )
        restore_inputs = cast("dict[str, object]", restore["with"])
        save_inputs = cast("dict[str, object]", save["with"])
        assert "if" not in restore
        assert str(restore_inputs["path"]).splitlines() == [
            "carddb/.mypy_cache",
            "publish/.mypy_cache",
        ]
        assert save_inputs["path"] == restore_inputs["path"]
        for project in ("carddb", "publish"):
            for filename in ("uv.lock", "pyproject.toml"):
                for cache_key in ("key", "restore-keys"):
                    assert f"'{project}/{filename}'" in str(restore_inputs[cache_key])
        web_steps = cast("list[dict[str, object]]", jobs["web"]["steps"])
        install = next(
            step
            for step in web_steps
            if step.get("run") == "bun install --frozen-lockfile"
        )
        assert "if" not in install

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

    def test_rust_uses_public_inputs_in_every_scope(self) -> None:
        """Data edits exercise consumers without private credentials or exclusions."""
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        steps = workflow["jobs"]["rust"]["steps"]
        checkouts = [
            step for step in steps if "actions/checkout@" in str(step.get("uses", ""))
        ]
        assert len(checkouts) == 1
        assert all("ssh-key" not in step.get("with", {}) for step in checkouts)
        tests = next(step for step in steps if step.get("id") == "tests")
        assert "rust_tests.py" in tests["run"]
        assert "--fail-under-lines 90" in tests["run"]
        filters = next(
            step
            for step in workflow["jobs"]["changes"]["steps"]
            if step.get("id") == "filter"
        )["with"]["filters"]
        rules = yaml.safe_load(filters)
        for component in ("shared", "python", "web"):
            assert not any(
                fnmatchcase("card-data/jp/SYN/000-099.json", pattern)
                for pattern in rules[component]
            )
        assert "card-data/**" in rules["card_data"]
        for path in (
            "card-data/jp/SYN/000-099.json",
            "carddb/src/sve_carddb/core/json.py",
            "carddb/src/sve_carddb/domains/parsed_cards/engine_fixture.py",
            "carddb/src/sve_carddb/parse/pages/extract_jp.py",
            "carddb/pyproject.toml",
            "carddb/uv.lock",
            "tests/engine/card-input.json",
        ):
            assert any(
                fnmatchcase(path, pattern)
                for pattern in (*rules["shared"], *rules["rust"])
            )

    def test_mutation_hook_generates_the_same_public_input(self) -> None:
        """Mutation baseline tests need the same generated catalogue as coverage."""
        repos = cast("list[dict[str, object]]", load_config()["repos"])
        hook = next(
            hook
            for repo in repos
            for hook in cast("list[dict[str, object]]", repo["hooks"])
            if hook["id"] == "cargo-mutants"
        )
        assert (
            hook["entry"]
            == "python3 .github/ci/rust_tests.py mutants --workspace -j 4 --timeout 120"
        )

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
