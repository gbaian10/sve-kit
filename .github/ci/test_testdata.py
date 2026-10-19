# ruff: file-ignore[no-self-use, magic-value-comparison, pytest-unittest-raises-assertion] -- standalone unittest and fixed protocol oracles
"""Exercise trusted/fork scopes and failures with independent synthetic inputs."""

import contextlib
import hashlib
import io
import json
import os
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- synthetic CompletedProcess values only
import tempfile
import unittest
from pathlib import Path
from typing import override
from unittest.mock import patch

import testdata

REPOSITORY = "owner/project"
COMMIT = "a" * 40
BODY = b"synthetic private page"


def event(head: str = REPOSITORY) -> dict[str, object]:
    """Use GitHub's repo provenance fields, independently of credentials."""
    return {
        "repository": {"full_name": REPOSITORY},
        "ref": "refs/heads/main",
        "pull_request": {
            "base": {"repo": {"full_name": REPOSITORY}},
            "head": {"repo": {"full_name": head}},
        },
    }


class TestdataTests(unittest.TestCase):
    """No private source files, tokens or network are needed by these tests."""

    @override
    def setUp(self) -> None:
        """Give every case isolated small pins and environment output files."""
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / ".github/ci").mkdir(parents=True)
        (self.root / "carddb/tests/fixtures").mkdir(parents=True)
        self.pin = self.root / ".github/ci/testdata.lock"
        self.pin.write_text(
            f'repo = "owner/private"\ncommit = "{COMMIT}"\n',
            encoding="utf-8",
        )
        (self.root / ".github/ci/fork-coverage.json").write_text(
            '{"python": 73}',
            encoding="utf-8",
        )
        (self.root / "carddb/tests/fixtures/private-pages.json").write_text(
            json.dumps(
                {
                    "commit": COMMIT,
                    "cases": [
                        {
                            "path": "fixtures/carddb/synthetic.html",
                            "sha256": hashlib.sha256(BODY).hexdigest(),
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )
        self.event_file = self.root / "event.json"
        self.event_file.write_text(json.dumps(event()), encoding="utf-8")
        self.environment = {
            "GITHUB_EVENT_NAME": "pull_request",
            "GITHUB_REPOSITORY": REPOSITORY,
            "GITHUB_EVENT_PATH": str(self.event_file),
            "GITHUB_ENV": str(self.root / "env"),
            "GITHUB_OUTPUT": str(self.root / "out"),
            "GITHUB_STEP_SUMMARY": str(self.root / "summary"),
        }
        root_patch = patch.object(testdata, "ROOT", self.root)
        root_patch.start()
        self.addCleanup(root_patch.stop)

    def test_full_and_fork_provenance_ignore_actor_and_missing_key(self) -> None:
        """Own bot/Dependabot PRs stay full; credential absence never selects fork."""
        for name in ("bot[bot]", "dependabot[bot]", "maintainer"):
            document = event()
            document["sender"] = {"login": name}
            assert testdata.scope("pull_request", REPOSITORY, document) == "full"
        assert testdata.scope("push", REPOSITORY, event()) == "full"
        assert (
            testdata.scope("pull_request", REPOSITORY, event("outside/project"))
            == "fork"
        )

    def test_missing_unknown_or_mismatched_provenance_is_rejected(self) -> None:
        """A malformed event must not acquire the secret-free success path."""
        for name, document in (
            ("workflow_dispatch", event()),
            ("push", {"repository": {"full_name": REPOSITORY}}),
            ("pull_request", {}),
            ("pull_request", {**event(), "pull_request": {}}),
            ("pull_request", {**event(), "repository": {"full_name": "other/project"}}),
        ):
            with (
                self.subTest(name=name, document=document),
                self.assertRaises(ValueError),
            ):
                testdata.scope(name, REPOSITORY, document)

    def test_fork_configuration_does_not_read_private_files(self) -> None:
        """Forks explicitly exclude private tests even when a key happens to exist."""
        self.event_file.write_text(
            json.dumps(event("outside/project")), encoding="utf-8"
        )
        with (
            patch.dict(os.environ, self.environment, clear=True),
            patch.object(subprocess, "run") as execute,
        ):
            testdata.configure("python")
        execute.assert_not_called()
        assert (self.root / "env").read_text(encoding="utf-8") == (
            "SVE_PRIVATE_TESTDATA_MODE=excluded\nSVE_CI_TEST_MODE=fork\nSVE_CI_COVERAGE_THRESHOLD=73\n"
        )
        assert "mode=excluded\n" in (self.root / "out").read_text(encoding="utf-8")
        assert not (self.root / ".testdata").exists()
        summary = (self.root / "summary").read_text(encoding="utf-8")
        assert "私有真實頁測試未執行" in summary
        assert "not full coverage acceptance" in summary

    def test_full_configuration_requires_private_testdata(self) -> None:
        """Full scope keeps the existing 90% gate and an explicit required mode."""
        with patch.dict(os.environ, self.environment, clear=True):
            testdata.configure("python")
        assert (self.root / "env").read_text(encoding="utf-8") == (
            "SVE_PRIVATE_TESTDATA_MODE=required\nSVE_CI_TEST_MODE=full\nSVE_CI_COVERAGE_THRESHOLD=90\n"
        )

    def test_pin_disagreement_and_unsafe_paths_fail(self) -> None:
        """A malformed repository or checkout pin mismatch cannot reach checkout."""
        original = self.pin.read_text(encoding="utf-8")
        for changed in (
            original.replace(COMMIT, "b" * 40),
            original.replace("owner/private", "../private"),
        ):
            self.pin.write_text(changed, encoding="utf-8")
            with self.assertRaises(ValueError):
                testdata.pin()

    def test_fork_thresholds_are_independent_and_invalid_values_fail(self) -> None:
        """Never silently use full coverage or disable a fork gate."""
        assert testdata.threshold("python", "fork") == 73
        assert testdata.threshold("python", "full") == 90
        for value in (0, True, "73", 101):
            (self.root / ".github/ci/fork-coverage.json").write_text(
                json.dumps({"python": value}), encoding="utf-8"
            )
            with self.assertRaises(ValueError):
                testdata.threshold("python", "fork")
        with self.assertRaises(ValueError):
            testdata.threshold("python", "unknown")

    def test_private_verification_is_trusted_and_exact(self) -> None:
        """The checked-out commit and private bytes are separate fail-closed checks."""
        snapshot = self.root / ".testdata/fixtures/carddb/synthetic.html"
        snapshot.parent.mkdir(parents=True)
        snapshot.write_bytes(BODY)
        success = subprocess.CompletedProcess([], 0, stdout=COMMIT + "\n")
        with (
            patch.dict(
                os.environ,
                {**self.environment, "SVE_PRIVATE_TESTDATA_MODE": "required"},
                clear=True,
            ),
            patch.object(subprocess, "run", return_value=success),
        ):
            testdata.verify("python")
            snapshot.write_bytes(b"changed synthetic input")
            with self.assertRaises(ValueError):
                testdata.verify("python")
        with patch.dict(
            os.environ, {"SVE_PRIVATE_TESTDATA_MODE": "excluded"}, clear=True
        ):
            with self.assertRaises(ValueError):
                testdata.verify("python")

    def test_wrong_private_commit_fails_without_disclosing_bytes(self) -> None:
        """Tool errors stay behind a safe CLI diagnostic."""
        result = subprocess.CompletedProcess([], 0, stdout="b" * 40)
        output = io.StringIO()
        with (
            patch.dict(
                os.environ, {"SVE_PRIVATE_TESTDATA_MODE": "required"}, clear=True
            ),
            patch.object(subprocess, "run", return_value=result),
            contextlib.redirect_stderr(output),
        ):
            assert testdata.main(["verify", "python"]) == 1
        assert (
            output.getvalue()
            == "Test scope or private test-data verification failed.\n"
        )

    def test_private_inventory_rejects_escape_and_duplicates(self) -> None:
        """A valid checkout cannot authorize paths outside its fixture root."""
        index = self.root / "carddb/tests/fixtures/private-pages.json"
        for path in ("../private", "/absolute", "fixtures/carddb/../outside"):
            index.write_text(
                json.dumps({"cases": [{"path": path, "sha256": "a" * 64}]})
            )
            with self.assertRaises(ValueError):
                testdata.private_files()
        case = {"path": "fixtures/carddb/synthetic.html", "sha256": "a" * 64}
        index.write_text(json.dumps({"cases": [case, case]}))
        with self.assertRaises(ValueError):
            testdata.private_files()
