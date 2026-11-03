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
FULL_THRESHOLD = 90
# python and rust summarise one CI shard; the -coverage kinds gate the merged shards.
KINDS = ("python", "publish", "web", "rust", "python-coverage", "rust-coverage")


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


def without_file_group(raw: str) -> str:
    """Drop only xdist's file group, keeping parameters available to privacy filters."""
    name, separator, group = raw.rpartition("@")
    if separator and re.fullmatch(r"tests/[A-Za-z0-9_./-]+\.py", group):
        return name
    return raw


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
        name = without_file_group(node.get("name", "")).split("[", 1)[0]
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


def coverage_scope(kind: str = "python") -> tuple[str, float]:
    """Return the fixed acceptance gate for each test component."""
    if kind == "publish":
        return "synthetic", 92
    if kind in {"python", "rust"}:
        return "full", FULL_THRESHOLD
    raise ValueError("Unknown test component")


def scope_description(kind: str = "python") -> str:
    """Describe the tests and their acceptance scope."""
    if kind == "rust":
        return "Test scope: **full**; generated public card input, identical for project and fork PRs."
    if kind == "publish":
        return "Test scope: **synthetic publish tests**; no private test data required."
    return "Test scope: **full**; public SVE fixtures and synthetic cases run for every PR."


def coverage_description(folder: Path, *, rust: bool, kind: str = "python") -> str:
    """Keep failure counts visible even when failing tests prevented coverage export."""
    _, minimum = coverage_scope(kind)
    label = "Line" if rust else "Combined line + branch"
    try:
        percent = coverage(folder / "coverage.json", rust=rust)
    except (OSError, ValueError, TypeError, KeyError, IndexError):
        return f"{label} coverage unavailable; required **{minimum:g}%**."
    return f"{label} coverage: **{percent:.2f}%**, required **{minimum:g}%**."


def failure_section(locations: list[str]) -> list[str]:
    """List every safe failure location, independently of the slowest-case cutoff."""
    return [
        "",
        "### Failed tests",
        "",
        *(f"- `{location}`" for location in dict.fromkeys(locations)),
        *([] if locations else ["None reported."]),
    ]


def shard_report(path: Path) -> dict[str, int]:
    """Read one shard's index, shard count, collected and selected test counts."""
    report = json_object(path)
    values = {key: report[key] for key in ("index", "total", "collected", "selected")}
    if not all(type(value) is int and value >= 0 for value in values.values()):
        msg = "Invalid shard report"
        raise ValueError(msg)
    return cast("dict[str, int]", values)


def shard_description(folder: Path, ran: int) -> tuple[str, bool]:
    """Every test this shard selected must have reported a result."""
    try:
        report = shard_report(folder / "shard.json")
    except (OSError, ValueError, TypeError, KeyError):
        return "Shard counts unavailable; the coverage job will fail.", False
    complete = ran == report["selected"]
    return (
        f"Shard **{report['index']}/{report['total']}**: ran **{ran}** of the "
        f"**{report['selected']}** tests assigned here ({report['collected']} collected in total)"
        + ("." if complete else "; **some assigned tests did not report a result**."),
        complete,
    )


def partition_description(folder: Path) -> tuple[str, bool]:
    """The shards must be one complete set that ran every collected test exactly once."""
    reports = [shard_report(path) for path in sorted(folder.glob("shard-*.json"))]
    if not reports:
        msg = "No shard reports"
        raise ValueError(msg)
    total = reports[0]["total"]
    collected = reports[0]["collected"]
    selected = sum(report["selected"] for report in reports)
    complete = (
        sorted(report["index"] for report in reports) == list(range(1, total + 1))
        and all(report["total"] == total for report in reports)
        and all(report["collected"] == collected for report in reports)
        and selected == collected
    )
    return (
        f"Shards: **{len(reports)}** of **{total}** · collected **{collected}** · "
        f"run across shards **{selected}**"
        + (
            " (every collected test ran exactly once)."
            if complete
            else "; **the shards do not add up to the collected tests**."
        ),
        complete,
    )


def combined_summary(kind: str, folder: Path) -> tuple[str, int]:
    """Check the shard partition and the full coverage gate on the merged data."""
    component = kind.removesuffix("-coverage")
    rust = component == "rust"
    _, minimum = coverage_scope(component)
    label = "Line" if rust else "Combined line + branch"
    partition, complete = partition_description(folder)
    # python-coverage writes coverage.json with `coverage json`, rust_coverage.py with llvm-cov.
    percent = coverage(folder / "coverage.json", rust=rust)
    lines = [
        f"## {component.title()} coverage (all shards)",
        "",
        scope_description(component),
        partition,
        f"{label} coverage: **{percent:.2f}%**, required **{minimum:g}%**.",
    ]
    return "\n".join(lines) + "\n", int(not complete or percent < minimum)


def junit_summary(kind: str, folder: Path) -> str:
    """Render counts, timings and safe locations for Python or Web."""
    cases, elapsed = junit(folder / "junit.xml", python=kind in {"python", "publish"})
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
    if kind == "publish":
        lines.extend(
            [
                scope_description(kind),
                coverage_description(folder, rust=False, kind=kind),
            ]
        )
    if kind == "python":
        lines.append(shard_description(folder, len(cases))[0])
    lines.extend(
        failure_section(
            [f"{case.file}::{case.name}" for case in cases if case.state == "failed"]
        )
    )
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


RUST_RESULT = re.compile(
    r"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; .*?finished in ([0-9.]+)s$",
    re.MULTILINE,
)


def rust_results(folder: Path) -> tuple[str, list[tuple[str, str, str, str]]]:
    """Return the raw log and its stable libtest result lines (passed, failed, ignored, seconds)."""
    source = (folder / "test.log").read_text(encoding="utf-8")
    results = RUST_RESULT.findall(source)
    if not results:
        msg = "No Rust test result lines"
        raise ValueError(msg)
    return source, results


def ran(kind: str, folder: Path) -> int:
    """Count every test of this shard that reported a result, including failures and skips."""
    if kind == "rust":
        _, results = rust_results(folder)
        return sum(int(row[0]) + int(row[1]) + int(row[2]) for row in results)
    return len(junit(folder / "junit.xml", python=True)[0])


# Cargo names each binary it runs; only repository paths and crate names reach the summary.
RUST_BINARY = re.compile(
    r"^\s*Running (?:unittests )?([A-Za-z0-9_./-]+\.rs) \(.*?/deps/([A-Za-z0-9_-]+?)-[0-9a-f]+\)$"
)


def binary_section(source: str) -> list[str]:
    """Time each test binary, so per-binary setup costs stay visible across shards."""
    rows: list[str] = []
    label = None
    for line in source.splitlines():
        if match := RUST_BINARY.match(line):
            label = f"{match[2]} ({match[1]})"
        elif label and (result := RUST_RESULT.match(line)):
            ran = int(result[1]) + int(result[2]) + int(result[3])
            rows.append(f"| {label} | {ran} | {number(result[4]):.2f} |")
            label = None
    return [
        "",
        "### Test binaries",
        "",
        "| Binary | Tests run | Seconds |",
        "| --- | ---: | ---: |",
        *rows,
    ]


def rust_summary(folder: Path) -> str:
    """Aggregate stable libtest result lines without exposing panic output."""
    source, results = rust_results(folder)
    passed = sum(int(row[0]) for row in results)
    failed = sum(int(row[1]) for row in results)
    skipped = sum(int(row[2]) for row in results)
    elapsed = number((folder / "elapsed.txt").read_text(encoding="utf-8").strip())
    seconds = sum(number(row[3]) for row in results)
    shard_line, _ = shard_description(folder, passed + failed + skipped)
    locations = re.findall(
        r"^test ((?:r#)?[A-Za-z_][A-Za-z0-9_]*(?:::(?:r#)?[A-Za-z_][A-Za-z0-9_]*)*) \.\.\. FAILED$",
        source,
        re.MULTILINE,
    )
    failures = "\n".join(failure_section(locations) + binary_section(source))
    return f"## Rust tests\n\nTotal: **{passed + failed + skipped}** · Passed: **{passed}** · Skipped/ignored: **{skipped}** · Failed: **{failed}**\nWall time (including build/coverage): **{elapsed:.2f} s** · Summed test-binary time: **{seconds:.2f} s**\n{shard_line}\n{failures}\n"


def main(argv: list[str]) -> int:
    """Append safe statistics to the job summary, including an explicit missing-report failure."""
    if len(argv) != EXPECTED_ARGS or argv[0] not in KINDS:
        sys.stderr.write(f"Usage: test_summary.py {'|'.join(KINDS)} REPORT_DIRECTORY\n")
        return 2
    kind, directory = argv
    status = 0
    try:
        folder = Path(directory)
        if kind.endswith("-coverage"):
            summary, status = combined_summary(kind, folder)
        elif kind == "rust":
            summary = rust_summary(folder)
        else:
            summary = junit_summary(kind, folder)
        if kind in {"python", "rust"}:
            status = int(not shard_description(folder, ran(kind, folder))[1])
        if kind == "publish":
            try:
                _, minimum = coverage_scope(kind)
                status = int(coverage(folder / "coverage.json", rust=False) < minimum)
            except (OSError, ValueError, TypeError, KeyError, IndexError):
                status = 1
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
