"""Rewrite pytest JUnit into a copy that may leave the runner: no parameters, messages or output."""

import hashlib
import re
import sys
import xml.etree.ElementTree as ET  # ruff: ignore[suspicious-xml-etree-import] -- local reports; DTDs rejected before parsing
from pathlib import Path

from test_summary import number

EXPECTED_ARGS = 3
MODULE = re.compile(
    r"^(tests(?:\.[A-Za-z_][A-Za-z0-9_]*)*\.test_[A-Za-z0-9_]+)(?:\.(Test[A-Za-z0-9_]*))?(?:\.|$)"
)
FUNCTION = re.compile(r"test_[A-Za-z0-9_]+")


def short_hash(text: str) -> str:
    """A stable id for a case: the same parameters always map to the same eight characters."""
    return hashlib.sha256(text.encode()).hexdigest()[:8]


def public_name(raw: str) -> str:
    """Keep the function name; parameters (which may hold card wording) become a hash."""
    function, bracket, parameters = raw.partition("[")
    if not FUNCTION.fullmatch(function):
        return f"case_{short_hash(raw)}"
    return f"{function}[{short_hash(bracket + parameters)}]" if bracket else function


def public_classname(raw: str) -> str:
    """Only the test module and class, never an arbitrary reporter label."""
    match = MODULE.match(raw)
    if match is None:
        return "unknown"
    return f"{match[1]}.{match[2]}" if match[2] else match[1]


def public_junit(source: Path, destination: Path) -> int:
    """Write names, times and pass/fail only; failure text and captured output are dropped."""
    text = source.read_text(encoding="utf-8")
    if "<!DOCTYPE" in text or "<!ENTITY" in text:
        msg = "Report DTDs are forbidden"
        raise ValueError(msg)
    root = ET.fromstring(text)  # ruff: ignore[suspicious-xml-element-tree-usage] -- rejects DTDs and entities above
    suite = ET.Element("testsuite", name="pytest")
    failures = errors = skipped = total = 0
    seconds = 0.0
    for node in root.iter("testcase"):
        elapsed = number(node.get("time", "0"))
        case = ET.SubElement(
            suite,
            "testcase",
            classname=public_classname(node.get("classname", "")),
            name=public_name(node.get("name", "")),
            time=f"{elapsed:.6f}",
        )
        total += 1
        seconds += elapsed
        if node.find("failure") is not None:
            ET.SubElement(case, "failure", message="failed")
            failures += 1
        elif node.find("error") is not None:
            ET.SubElement(case, "error", message="error")
            errors += 1
        elif node.find("skipped") is not None:
            ET.SubElement(case, "skipped")
            skipped += 1
    suite.set("tests", str(total))
    suite.set("failures", str(failures))
    suite.set("errors", str(errors))
    suite.set("skipped", str(skipped))
    suite.set("time", f"{seconds:.6f}")
    wrapper = ET.Element("testsuites")
    wrapper.append(suite)
    ET.indent(wrapper)
    ET.ElementTree(wrapper).write(destination, encoding="utf-8", xml_declaration=True)
    return total


def main(argv: list[str]) -> int:
    """Print only the case count, never a name."""
    if len(argv) != EXPECTED_ARGS:
        sys.stderr.write("Usage: junit_public.py SOURCE_JUNIT DESTINATION_JUNIT\n")
        return 2
    try:
        total = public_junit(Path(argv[1]), Path(argv[2]))
    except (OSError, ET.ParseError, ValueError):
        # The exception text can quote the offending report field, which must not reach the log.
        sys.stderr.write("Unable to sanitize the test report\n")
        return 1
    sys.stdout.write(f"Wrote {total} sanitized test cases\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
