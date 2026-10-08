# ruff: file-ignore[pytest-unittest-raises-assertion] -- repo CI intentionally installs dev dependencies only, not pytest
"""Regression tests for counts, partial failures and private-report disclosure."""

import contextlib
import io
import json
import os
import tempfile
import unittest
from pathlib import Path
from typing import override
from unittest.mock import patch

from test_summary import coverage_scope, junit, junit_summary, main, rust_summary

PRIVATE = "合成卡文_PRIVATE_SENTINEL"


class SummaryTests(unittest.TestCase):
    """Statistics must remain useful while report messages and parameters stay private."""

    @override
    def setUp(self) -> None:
        """Use a new small synthetic report directory per test."""
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.folder = Path(temporary.name)

    def write_junit(self) -> None:
        """Cover nested suites, parameters, errors, failures and skips."""
        (self.folder / "junit.xml").write_text(
            f"""<testsuites><testsuite time="9.5">
<testcase classname="tests.test_alpha.TestGroup" name="test_ok[{PRIVATE}]" time="1"/>
<testcase classname="tests.test_alpha" name="test_fail" time="2"><failure message="{PRIVATE}">{PRIVATE}</failure><system-out>{PRIVATE}</system-out></testcase>
<testcase classname="tests.test_beta" name="test_error" time="3"><error>{PRIVATE}</error></testcase>
<testcase classname="tests.test_beta" name="test_skip" time="0"><skipped message="{PRIVATE}"/></testcase>
<testcase classname="tests.test_beta" name="test_other" time="0.5"/>
</testsuite></testsuites>""",
            encoding="utf-8",
        )
        (self.folder / "coverage.json").write_text(
            json.dumps({"totals": {"percent_covered": 91.25}}), encoding="utf-8"
        )

    def test_python_counts_locations_and_timings(self) -> None:
        """Count testcases once, combine errors with failures and omit private fields."""
        self.write_junit()
        summary = junit_summary("python", self.folder)
        assert (
            "Total: **5** · Passed: **2** · Skipped: **1** · Failed/errors: **2**"
            in summary
        )
        assert "**9.50 s**" in summary
        assert "**91.25%**" in summary
        assert "| tests/test_alpha.py | 2 | 3.000 |" in summary
        assert "| tests/test_beta.py | 3 | 3.500 |" in summary
        slowest = summary.split("### Slowest 30 cases", 1)[1]
        assert slowest.index("tests/test_beta.py::test_error") < slowest.index(
            "tests/test_alpha.py::test_fail"
        )
        assert PRIVATE not in summary
        assert "[" not in summary

    def test_publish_keeps_its_gate_in_fork_mode(self) -> None:
        """Publisher coverage never borrows carddb's remaining-test threshold."""
        self.write_junit()
        with patch.dict(
            os.environ,
            {"SVE_CI_TEST_MODE": "fork", "SVE_CI_COVERAGE_THRESHOLD": "70"},
        ):
            assert coverage_scope("publish") == ("synthetic", 92)
            summary = junit_summary("publish", self.folder)
            assert "required **92%**" in summary
            assert "no private test data required" in summary
            assert "tests/test_alpha.py::test_fail" in summary
            assert PRIVATE not in summary
            with contextlib.redirect_stdout(io.StringIO()):
                assert main(["publish", str(self.folder)]) == 1
                (self.folder / "coverage.json").write_text(
                    json.dumps({"totals": {"percent_covered": 92.5}}),
                    encoding="utf-8",
                )
                assert main(["publish", str(self.folder)]) == 0

    def test_fork_coverage_uses_its_own_gate_and_disclosure(self) -> None:
        """The same report can fail the full gate and pass the remaining-test gate."""
        self.write_junit()
        (self.folder / "coverage.json").write_text(
            json.dumps({"totals": {"percent_covered": 74.5}}), encoding="utf-8"
        )
        for mode, threshold, expected in (
            ("full", "90", 1),
            ("fork", "73", 0),
            ("fork", "75", 1),
        ):
            output = io.StringIO()
            with (
                patch.dict(
                    os.environ,
                    {"SVE_CI_TEST_MODE": mode, "SVE_CI_COVERAGE_THRESHOLD": threshold},
                    clear=True,
                ),
                contextlib.redirect_stdout(output),
            ):
                assert main(["python", str(self.folder)]) == expected
            summary = output.getvalue()
            assert f"required **{threshold}%**" in summary
            assert "Failed/errors: **2**" in summary
            assert PRIVATE not in summary
            if mode == "fork":
                assert "私有真實頁測試未執行" in summary
                assert "This is not full coverage acceptance" in summary
            else:
                assert "Test scope: **full**" in summary

    def test_invalid_scope_or_threshold_never_falls_back_to_full(self) -> None:
        """Unknown flags and a missing fork gate must fail closed."""
        for environment in (
            {"SVE_CI_TEST_MODE": "fork"},
            {"SVE_CI_TEST_MODE": "unknown", "SVE_CI_COVERAGE_THRESHOLD": "73"},
            {"SVE_CI_TEST_MODE": "full", "SVE_CI_COVERAGE_THRESHOLD": "73"},
            {"SVE_CI_TEST_MODE": "fork", "SVE_CI_COVERAGE_THRESHOLD": "NaN"},
            {"SVE_CI_TEST_MODE": "fork", "SVE_CI_COVERAGE_THRESHOLD": "0"},
        ):
            with patch.dict(os.environ, environment, clear=True):
                with self.assertRaises(ValueError):
                    coverage_scope()

    def test_missing_fork_coverage_fails_but_preserves_safe_counts(self) -> None:
        """A test crash cannot make the remaining-test summary a green gate."""
        self.write_junit()
        (self.folder / "coverage.json").unlink()
        output = io.StringIO()
        with (
            patch.dict(
                os.environ,
                {"SVE_CI_TEST_MODE": "fork", "SVE_CI_COVERAGE_THRESHOLD": "73"},
                clear=True,
            ),
            contextlib.redirect_stdout(output),
        ):
            assert main(["python", str(self.folder)]) == 1
        assert "Failed/errors: **2**" in output.getvalue()
        assert "coverage unavailable; required **73%**" in output.getvalue()
        assert PRIVATE not in output.getvalue()

    def test_web_labels_and_malformed_locations_stay_private(self) -> None:
        """Web description strings and unsafe paths are never summary labels."""
        (self.folder / "junit.xml").write_text(
            f'<testsuite time="2"><testcase classname="/work/sim/web/src/check.test.tsx" name="{PRIVATE}" time="1"/><testcase classname="tests/../{PRIVATE}" name="{PRIVATE}" time="1"/></testsuite>',
            encoding="utf-8",
        )
        summary = junit_summary("web", self.folder)
        assert "src/check.test.tsx::case 1" in summary
        assert "unknown test file::case 2" in summary
        assert PRIVATE not in summary

    def test_failure_without_coverage_preserves_counts(self) -> None:
        """Test failure diagnostics stay available numerically before coverage export."""
        self.write_junit()
        (self.folder / "coverage.json").unlink()
        summary = junit_summary("python", self.folder)
        assert "Failed/errors: **2**" in summary
        assert "coverage unavailable; required **90%**" in summary

    def test_missing_malformed_and_empty_report_fail_safely(self) -> None:
        """Missing reports cannot turn a test crash into a successful summary."""
        for report in (None, f"broken {PRIVATE}", "<testsuites/>"):
            if report is not None:
                (self.folder / "junit.xml").write_text(report, encoding="utf-8")
            output = io.StringIO()
            target = self.folder / "summary.md"
            with (
                patch.dict(os.environ, {"GITHUB_STEP_SUMMARY": str(target)}),
                contextlib.redirect_stdout(output),
            ):
                assert main(["python", str(self.folder)]) == 1
            assert "Statistics unavailable" in output.getvalue()
            assert PRIVATE not in output.getvalue()
            assert PRIVATE not in target.read_text(encoding="utf-8")

    def test_entity_and_nonfinite_times_rejected(self) -> None:
        """DTD/entity expansion and invalid times cannot reach the summary."""
        for report in (
            '<!DOCTYPE x [<!ENTITY a "private">]><testsuite/>',
            '<testsuite><testcase time="NaN"/></testsuite>',
            '<testsuite><testcase time="-1"/></testsuite>',
        ):
            (self.folder / "junit.xml").write_text(report, encoding="utf-8")
            with self.assertRaises(ValueError):
                junit(self.folder / "junit.xml", python=True)

    def test_slowest_thirty_and_suite_time(self) -> None:
        """Slowest cases are ranked by case time, without summing parallel elapsed."""
        cases = "".join(
            f'<testcase classname="tests.test_rank" name="test_case_{i}" time="{i}"/>'
            for i in range(35)
        )
        (self.folder / "junit.xml").write_text(
            f'<testsuites time="40"><testsuite time="99">{cases}</testsuite></testsuites>',
            encoding="utf-8",
        )
        summary = junit_summary("python", self.folder)
        assert "**40.00 s**" in summary
        assert "::test_case_34 | 34.000" in summary
        assert "::test_case_5 | 5.000" in summary
        assert "::test_case_4 |" not in summary
        assert "| tests/test_rank.py | 35 | 595.000 |" in summary
        assert "### Failed tests\n\nNone reported." in summary

    def test_fast_python_failure_and_error_outside_slowest_thirty(self) -> None:
        """Failures must be findable even when thirty passing cases are slower."""
        passed = "".join(
            f'<testcase classname="tests.test_rank" name="test_pass_{i}" time="10"/>'
            for i in range(35)
        )
        failed = f'<testcase classname="tests.test_fast" name="test_failure[{PRIVATE}]" time="0"><failure message="{PRIVATE}">{PRIVATE}</failure></testcase>'
        error = f'<testcase classname="tests.test_fast" name="test_error" time="0"><error>{PRIVATE}</error><system-err>{PRIVATE}</system-err></testcase>'
        (self.folder / "junit.xml").write_text(
            f"<testsuite>{passed}{failed}{error}</testsuite>", encoding="utf-8"
        )
        summary = junit_summary("python", self.folder)
        failures, slowest = summary.split("### Slowest 30 cases", 1)
        assert "- `tests/test_fast.py::test_failure`" in failures
        assert "- `tests/test_fast.py::test_error`" in failures
        assert "::test_failure" not in slowest
        assert "::test_error" not in slowest
        assert PRIVATE not in summary

    def test_fast_web_failure_uses_file_and_ordinal(self) -> None:
        """A fast Vitest failure stays visible without publishing its description."""
        passed = "".join(
            f'<testcase classname="src/slow.test.ts" name="{PRIVATE}" time="10"/>'
            for _ in range(35)
        )
        failed = f'<testcase classname="src/fast.test.tsx" name="{PRIVATE}" time="0"><failure>{PRIVATE}</failure></testcase>'
        (self.folder / "junit.xml").write_text(
            f"<testsuite>{passed}{failed}</testsuite>", encoding="utf-8"
        )
        summary = junit_summary("web", self.folder)
        failures, slowest = summary.split("### Slowest 30 cases", 1)
        assert "- `src/fast.test.tsx::case 36`" in failures
        assert "::case 36" not in slowest
        assert PRIVATE not in summary

    def test_rust_counts_wall_time_and_missing_coverage(self) -> None:
        """Aggregate multiple binaries and doctests; never include panics or source."""
        (self.folder / "test.log").write_text(
            f"""panic: {PRIVATE}
test boundaries::test_failure ... FAILED
test module::r#type::test_error ... FAILED
test {PRIVATE} ... FAILED
test [unsafe](https://example.com) ... FAILED
test boundaries::test_ok ... ok
test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.20s
test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s
""",
            encoding="utf-8",
        )
        (self.folder / "elapsed.txt").write_text("12", encoding="utf-8")
        (self.folder / "coverage.json").write_text(
            json.dumps({"data": [{"totals": {"lines": {"percent": 95.5}}}]}),
            encoding="utf-8",
        )
        summary = rust_summary(self.folder)
        assert (
            "Total: **7** · Passed: **4** · Skipped/ignored: **1** · Failed: **2**"
            in summary
        )
        assert "**12.00 s**" in summary
        assert "**1.50 s**" in summary
        assert "**95.50%**" in summary
        assert "### Failed tests\n\n- `boundaries::test_failure`" in summary
        assert "- `module::r#type::test_error`" in summary
        assert "test_ok" not in summary
        assert "unsafe" not in summary
        assert PRIVATE not in summary
        (self.folder / "coverage.json").unlink()
        assert "coverage unavailable" in rust_summary(self.folder)
