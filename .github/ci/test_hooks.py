"""Exercise the actual workflow gate with representative GitHub needs results."""

import json
import os
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- runs the checked-in gate only
import unittest
from copy import deepcopy
from fnmatch import fnmatchcase
from typing import cast, override

import yaml

from hooks import ROOT, workflow_problems


class WorkflowGateTests(unittest.TestCase):
    """Fail closed for failed components, incomplete dependencies and unknown jobs."""

    @override
    def setUp(self) -> None:
        """Load the real workflow and start with a docs-only needs result."""
        self.workflow = cast(
            "dict[str, object]",
            yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text()),
        )
        self.jobs = cast("dict[str, dict[str, object]]", self.workflow["jobs"])
        steps = cast("list[dict[str, object]]", self.jobs["ci-ok"]["steps"])
        self.gate = str(steps[0]["run"])
        self.outputs = dict.fromkeys(("python", "rust", "web", "card_data"), "false")
        self.needs: dict[str, object] = {
            job: {
                "result": "success"
                if job in {"changes", "repo", "commit"}
                else "skipped"
            }
            for job in self.jobs
            if job != "ci-ok"
        }

    def run_gate(self) -> int:
        """Run the exact shell in ci.yml, including its jq component mapping."""
        self.needs["changes"] = {"result": "success", "outputs": self.outputs}
        result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed bash argv
            [shutil.which("bash") or "bash", "-eu", "-o", "pipefail", "-c", self.gate],
            env={**os.environ, "NEEDS": json.dumps(self.needs)},
            capture_output=True,
            check=False,
        )
        return result.returncode

    def test_docs_only(self) -> None:
        """All component jobs may skip when only docs changed."""
        assert self.run_gate() == 0

    def test_component_results(self) -> None:
        """Failure, cancellation and unexpected skipping must block every component."""
        for component, jobs in {
            "card_data": ("card-data",),
            "python": ("python",),
            "rust": ("rust",),
            "web": ("web",),
        }.items():
            for job in jobs:
                for result in ("failure", "cancelled", "skipped"):
                    with self.subTest(job=job, result=result):
                        self.setUp()
                        self.outputs[component] = "true"
                        for sibling in jobs:
                            self.needs[sibling] = {"result": "success"}
                        self.needs[job] = {"result": result}
                        assert self.run_gate() != 0

    def test_required_and_unknown_jobs(self) -> None:
        """Unmapped jobs never gain permission to skip."""
        for job in ("repo", "commit", "new-job"):
            with self.subTest(job=job):
                self.setUp()
                self.needs[job] = {"result": "skipped"}
                assert self.run_gate() != 0
                self.needs[job] = {"result": "success"}
                assert self.run_gate() == 0

    def test_missing_output_fails_closed(self) -> None:
        """Absent paths-filter outputs cannot excuse a skipped component."""
        self.outputs.clear()
        assert self.run_gate() != 0

    def test_failed_changes(self) -> None:
        """The gate cannot pass if the component decision failed."""
        needs = deepcopy(self.needs)
        needs["changes"] = {"result": "failure"}
        result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed bash argv
            [shutil.which("bash") or "bash", "-c", self.gate],
            env={**os.environ, "NEEDS": json.dumps(needs)},
            capture_output=True,
            check=False,
        )
        assert result.returncode != 0

    def test_every_job_is_a_dependency(self) -> None:
        """Adding a job without updating needs, or losing a dependency, fails check."""
        assert not workflow_problems(self.workflow)
        self.jobs["new-job"] = {}
        assert workflow_problems(self.workflow)
        needs = cast("list[str]", self.jobs["ci-ok"]["needs"])
        needs.append("new-job")
        assert not workflow_problems(self.workflow)
        for job in tuple(needs):
            with self.subTest(job=job):
                needs.remove(job)
                assert workflow_problems(self.workflow)
                needs.append(job)
        needs.append("repo")
        assert workflow_problems(self.workflow)

    def test_card_data_changes_include_admission_code_and_job(self) -> None:
        """Run real-data validation when its implementation or execution changes."""
        steps = cast("list[dict[str, object]]", self.jobs["changes"]["steps"])
        step = next(step for step in steps if step.get("id") == "filter")
        filters = yaml.safe_load(cast("dict[str, str]", step["with"])["filters"])
        patterns = cast("list[str]", filters["card_data"])
        for path in (
            "card-data/jp/SYN/000-099.json",
            "carddb/src/sve_carddb/domains/parsed_cards/storage.py",
            "carddb/src/sve_carddb/core/models.py",
            "carddb/pyproject.toml",
            "carddb/uv.lock",
            ".github/workflows/ci.yml",
            ".github/ci/hooks.py",
        ):
            with self.subTest(path=path):
                assert any(fnmatchcase(path, pattern) for pattern in patterns)

    def test_card_data_job_keeps_previous_when_checkout_is_missing(self) -> None:
        """A missing base must reach the validator, never disable history checks."""
        steps = cast("list[dict[str, object]]", self.jobs["card-data"]["steps"])
        command = str(next(step["run"] for step in steps if "run" in step))
        result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed bash argv executing the checked-in job with an argv-recording uv stand-in
            [
                shutil.which("bash") or "bash",
                "-eu",
                "-c",
                'uv() { printf "%s\\n" "$@"; }\n' + command,
            ],
            env={**os.environ, "GITHUB_WORKSPACE": "/nonexistent synthetic checkout"},
            capture_output=True,
            text=True,
            check=True,
        )
        argv = result.stdout.splitlines()
        assert argv[-3:] == [
            "/nonexistent synthetic checkout/card-data",
            "--previous",
            "/nonexistent synthetic checkout/.card-data-base/card-data",
        ]
