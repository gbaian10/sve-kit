"""Publish statistics without copying card wording from reports into Actions logs."""

import json
import math
import os
import re
import sys
import xml.etree.ElementTree as ET  # ruff: ignore[suspicious-xml-etree-import] -- local reports; DTDs rejected before parsing
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import cast

EXPECTED_ARGS = 2


@dataclass(frozen=True)
class Case:
    """Safe test location and numeric result, excluding parameters and diagnostics."""

    file: str
    name: str
    seconds: float
    state: str


def number(value: object) -> float:
    """Accept only finite nonnegative report numbers."""
    result = float(str(value))
    if not math.isfinite(result) or result < 0:
        msg = "Invalid report number"
        raise ValueError(msg)
    return result


def safe_file(value: str) -> str:
    """Only print test source paths, never arbitrary reporter labels."""
    match = re.search(
        r"(?:^|/)((?:tests?/|src/|scripts/)[A-Za-z0-9_./-]+\.(?:py|tsx?|jsx?))$", value
    )
    return match[1] if match and ".." not in match[1] else "unknown test file"


def junit(path: Path, *, python: bool) -> tuple[list[Case], float]:
    """Read pytest/Vitest JUnit while dropping all messages and captured output."""
    source = path.read_text(encoding="utf-8")
    if "<!DOCTYPE" in source or "<!ENTITY" in source:
        msg = "Report DTDs are forbidden"
        raise ValueError(msg)
    root = ET.fromstring(source)  # ruff: ignore[suspicious-xml-element-tree-usage] -- rejects DTDs and entities above
    cases: list[Case] = []
    for index, node in enumerate(root.iter("testcase"), 1):
        classname = node.get("classname", "")
        filename = node.get("file", classname)
        if python:
            module = re.match(
                r"^(tests(?:\.[A-Za-z_][A-Za-z0-9_]*)*\.test_[A-Za-z0-9_]+)(?:\.|$)",
                classname,
            )
            filename = module[1].replace(".", "/") + ".py" if module else filename
        name = node.get("name", "").split("[", 1)[0]
        label = (
            name
            if python and re.fullmatch(r"test_[A-Za-z0-9_]+", name)
            else f"case {index}"
        )
        state = (
            "failed"
            if node.find("failure") is not None or node.find("error") is not None
            else "skipped"
            if node.find("skipped") is not None
            else "passed"
        )
        cases.append(
            Case(safe_file(filename), label, number(node.get("time", "0")), state)
        )
    suites = [root] if root.tag == "testsuite" else list(root.findall("testsuite"))
    elapsed = (
        number(root.get("time"))
        if root.get("time") is not None
        else sum(number(suite.get("time", "0")) for suite in suites)
    )
    return cases, elapsed


def json_object(path: Path) -> dict[str, object]:
    """Validate the outer boundary of local coverage JSON."""
    value: object = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        msg = "Expected a coverage object"
        raise TypeError(msg)
    return cast("dict[str, object]", value)


def coverage(path: Path, *, rust: bool) -> float:
    """Read the combined Python percentage or Rust LLVM line percentage."""
    report = json_object(path)
    if rust:
        data = cast("list[dict[str, object]]", report["data"])
        totals = cast("dict[str, dict[str, object]]", data[0]["totals"])
        return number(totals["lines"]["percent"])
    totals_python = cast("dict[str, object]", report["totals"])
    return number(totals_python["percent_covered"])


def coverage_description(folder: Path, *, rust: bool) -> str:
    """Keep failure counts visible even when failing tests prevented coverage export."""
    label = "Line" if rust else "Combined line + branch"
    try:
        percent = coverage(folder / "coverage.json", rust=rust)
    except (OSError, ValueError, TypeError, KeyError, IndexError):
        return f"{label} coverage unavailable; required **90%**."
    return f"{label} coverage: **{percent:.2f}%**, required **90%**."


def junit_summary(kind: str, folder: Path) -> str:
    """Render counts, timings and safe locations for Python or Web."""
    cases, elapsed = junit(folder / "junit.xml", python=kind == "python")
    if not cases:
        msg = "Report has no test cases"
        raise ValueError(msg)
    counts = {
        state: sum(case.state == state for case in cases)
        for state in ("passed", "skipped", "failed")
    }
    lines = [
        f"## {kind.title()} tests",
        "",
        f"Total: **{len(cases)}** · Passed: **{counts['passed']}** · Skipped: **{counts['skipped']}** · Failed/errors: **{counts['failed']}**",
        f"JUnit suite elapsed: **{elapsed:.2f} s** (case times include setup/teardown; parallel case times overlap).",
    ]
    if (folder / "elapsed.txt").exists():
        wall = number((folder / "elapsed.txt").read_text(encoding="utf-8").strip())
        lines.append(
            f"Command wall time (including startup/coverage): **{wall:.2f} s**."
        )
    if kind == "python":
        lines.append(coverage_description(folder, rust=False))
    lines.extend(
        [
            "",
            "### Slowest 30 cases",
            "",
            "| Test location (parameters omitted) | Seconds | Result |",
            "| --- | ---: | --- |",
        ]
    )
    lines.extend(
        f"| {case.file}::{case.name} | {case.seconds:.3f} | {case.state} |"
        for case in sorted(cases, key=lambda case: case.seconds, reverse=True)[:30]
    )
    files: dict[str, list[Case]] = defaultdict(list)
    for case in cases:
        files[case.file].append(case)
    lines.extend(
        [
            "",
            "### Test files",
            "",
            "| File | Cases | Summed case seconds |",
            "| --- | ---: | ---: |",
        ]
    )
    for filename, group in sorted(files.items()):
        lines.append(
            f"| {filename} | {len(group)} | {sum(case.seconds for case in group):.3f} |"
        )
    return "\n".join(lines) + "\n"


def rust_summary(folder: Path) -> str:
    """Aggregate stable libtest result lines without exposing panic output."""
    pattern = re.compile(
        r"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; .*?finished in ([0-9.]+)s$",
        re.MULTILINE,
    )
    results = pattern.findall((folder / "test.log").read_text(encoding="utf-8"))
    if not results:
        msg = "No Rust test result lines"
        raise ValueError(msg)
    passed = sum(int(row[0]) for row in results)
    failed = sum(int(row[1]) for row in results)
    skipped = sum(int(row[2]) for row in results)
    elapsed = number((folder / "elapsed.txt").read_text(encoding="utf-8").strip())
    seconds = sum(number(row[3]) for row in results)
    coverage_line = coverage_description(folder, rust=True)
    return f"## Rust tests\n\nTotal: **{passed + failed + skipped}** · Passed: **{passed}** · Skipped/ignored: **{skipped}** · Failed: **{failed}**\nWall time (including build/coverage): **{elapsed:.2f} s** · Summed test-binary time: **{seconds:.2f} s**\n{coverage_line}\n"


def main(argv: list[str]) -> int:
    """Append safe statistics to the job summary, including an explicit missing-report failure."""
    if len(argv) != EXPECTED_ARGS or argv[0] not in {"python", "web", "rust"}:
        sys.stderr.write("Usage: test_summary.py python|web|rust REPORT_DIRECTORY\n")
        return 2
    kind, directory = argv
    status = 0
    try:
        folder = Path(directory)
        summary = (
            rust_summary(folder) if kind == "rust" else junit_summary(kind, folder)
        )
    except (OSError, ValueError, TypeError, KeyError, IndexError, ET.ParseError):
        # Exception strings can contain diagnostics from the report; keep them private too.
        summary = f"## {kind.title()} tests\n\nStatistics unavailable: missing or invalid report. See the test step exit status; reproduce locally for diagnostics.\n"
        status = 1
    if target := os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(target).open("a", encoding="utf-8") as output:
            output.write(summary)
    sys.stdout.write(summary)
    return status


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
