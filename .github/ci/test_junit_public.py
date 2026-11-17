# ruff: file-ignore[pytest-unittest-raises-assertion, no-self-use, magic-value-comparison] -- repo CI installs dev dependencies only, not pytest; unittest requires methods; synthetic counts are literal
"""The public JUnit copy keeps identity and outcome and nothing a report could leak."""

import contextlib
import io
import tempfile
import unittest
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

    def test_file_groups_keep_public_identity_and_parameter_privacy(self) -> None:
        """Scheduling groups cannot replace names or alter parameter hashes."""
        group = "@tests/workflows/test_authoring.py"
        assert public_name("test_plain" + group) == "test_plain"
        raw = f"test_a[{PRIVATE}@example]"
        assert public_name(raw + group) == public_name(raw)
        assert PRIVATE not in public_name(raw + group)
        assert public_name("test_a@" + PRIVATE).startswith("case_")
        assert public_name(PRIVATE + group) == public_name(PRIVATE)

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
