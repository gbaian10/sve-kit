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

from test_summary import junit, junit_summary, main, rust_summary

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
        assert summary.index("tests/test_beta.py::test_error") < summary.index(
            "tests/test_alpha.py::test_fail"
        )
        assert PRIVATE not in summary
        assert "[" not in summary

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

    def test_rust_counts_wall_time_and_missing_coverage(self) -> None:
        """Aggregate multiple binaries and doctests; never include panics or source."""
        (self.folder / "test.log").write_text(
            f"""panic: {PRIVATE}
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
        assert PRIVATE not in summary
        (self.folder / "coverage.json").unlink()
        assert "coverage unavailable" in rust_summary(self.folder)
