# ruff: file-ignore[pytest-unittest-raises-assertion, no-self-use, magic-value-comparison] -- repo CI installs dev dependencies only, not pytest; unittest requires methods; synthetic counts are literal
"""The public JUnit copy keeps identity and outcome and nothing a report could leak."""

import contextlib
import io
import math
import tempfile
import unittest
import xml.etree.ElementTree as ET  # ruff: ignore[suspicious-xml-etree-import] -- parses the synthetic output of this module
from pathlib import Path
from typing import override

from junit_public import main, public_junit, public_name

PRIVATE = "合成卡文_PRIVATE_SENTINEL"


class PublicJunitTests(unittest.TestCase):
    """Card wording may sit in parameters, failure messages and captured output."""

    @override
    def setUp(self) -> None:
        """A new small synthetic folder per test."""
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.folder = Path(temporary.name)
        self.source = self.folder / "junit.xml"
        self.destination = self.folder / "public.xml"

    def write_source(self) -> None:
        """Cover parameters, a class, a failure, an error, a skip and unexpected labels."""
        self.source.write_text(
            f"""<testsuites><testsuite time="9.5" hostname="{PRIVATE}" timestamp="{PRIVATE}">
<testcase classname="tests.domains.test_alpha" name="test_ok[{PRIVATE}-1]" time="1"/>
<testcase classname="tests.domains.test_alpha.TestGroup" name="test_plain" time="0.5"/>
<testcase classname="tests.test_beta" name="test_fail[{PRIVATE}]" time="2"><failure message="{PRIVATE}">{PRIVATE}</failure><system-out>{PRIVATE}</system-out></testcase>
<testcase classname="tests.test_beta" name="test_error" time="3"><error message="{PRIVATE}">{PRIVATE}</error></testcase>
<testcase classname="tests.test_beta" name="test_skip" time="0"><skipped message="{PRIVATE}"/></testcase>
<testcase classname="{PRIVATE}" name="{PRIVATE}" time="0.25"/>
<testcase classname="tests.test_gamma" name="test_extra" file="{PRIVATE}" line="{PRIVATE}" time="0.5"><properties><property name="{PRIVATE}" value="{PRIVATE}"/></properties><system-err>{PRIVATE}</system-err>{PRIVATE}</testcase>{PRIVATE}
</testsuite></testsuites>""",
            encoding="utf-8",
        )

    def test_nothing_private_survives(self) -> None:
        """The sentinel never reaches the output, whichever field carried it."""
        self.write_source()
        assert public_junit(self.source, self.destination) == 7
        assert PRIVATE not in self.destination.read_text(encoding="utf-8")

    def test_outcomes_and_totals_are_kept(self) -> None:
        """States, counts and times stay accurate for Codecov's statistics."""
        self.write_source()
        public_junit(self.source, self.destination)
        root = ET.parse(self.destination).getroot()  # ruff: ignore[suspicious-xml-element-tree-usage] -- output of this module
        suite = root.find("testsuite")
        assert suite is not None
        assert suite.attrib["tests"] == "7"
        assert suite.attrib["failures"] == "1"
        assert suite.attrib["errors"] == "1"
        assert suite.attrib["skipped"] == "1"
        assert math.isclose(float(suite.attrib["time"]), 7.25)
        cases = {c.attrib["name"].split("[")[0]: c for c in suite.iter("testcase")}
        failure = cases["test_fail"].find("failure")
        assert failure is not None
        assert failure.attrib["message"] == "failed"
        assert cases["test_error"].find("error") is not None
        assert cases["test_skip"].find("skipped") is not None
        assert cases["test_plain"].attrib["classname"] == (
            "tests.domains.test_alpha.TestGroup"
        )

    def test_identity_is_stable_and_distinguishes_parameters(self) -> None:
        """Flaky detection needs the same case to keep the same name across runs."""
        assert public_name("test_a[x]") == public_name("test_a[x]")
        assert public_name("test_a[x]") != public_name("test_a[y]")
        assert public_name("test_a") == "test_a"
        assert public_name("test_a[x]").startswith("test_a[")
        assert public_name(PRIVATE).startswith("case_")
        assert PRIVATE not in public_name(PRIVATE)

    def test_dtds_are_rejected(self) -> None:
        """Local reports never need entities, so a DTD is an error."""
        self.source.write_text("<!DOCTYPE x><testsuites/>", encoding="utf-8")
        with self.assertRaises(ValueError):
            public_junit(self.source, self.destination)

    def run_main(self, *arguments: str) -> tuple[int, str]:
        """Run the command line and return the exit status with everything it printed."""
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            status = main(["junit_public.py", *arguments])
        return status, output.getvalue()

    def test_main_prints_a_count_only(self) -> None:
        """The command line reports how many cases, never which."""
        self.write_source()
        status, printed = self.run_main(str(self.source), str(self.destination))
        assert status == 0
        assert PRIVATE not in printed
        assert "7" in printed
        assert self.run_main()[0] == 2

    def test_errors_never_quote_the_report(self) -> None:
        """A malformed field must fail with fixed text, not the value float() would echo."""
        self.source.write_text(
            f'<testsuites><testcase name="test_a" time="{PRIVATE}"/></testsuites>',
            encoding="utf-8",
        )
        status, printed = self.run_main(str(self.source), str(self.destination))
        assert status == 1
        assert PRIVATE not in printed
        assert not self.destination.exists()


if __name__ == "__main__":
    unittest.main()
