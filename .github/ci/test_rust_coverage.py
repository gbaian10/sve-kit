# ruff: file-ignore[magic-value-comparison, pytest-unittest-raises-assertion] -- standalone unittest (no pytest) with synthetic LLVM tools
"""Exercise shard packaging and the merged gate with synthetic LLVM tools."""

import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from typing import override
from unittest.mock import patch

import rust_coverage

FILTER = "/rustc/([0-9a-f]+)/|^/w(/.*)?/(tests|examples|benches)/"
LOG = f"""\
   Compiling sve-engine v0.1.0
     Running `/sysroot/bin/llvm-profdata merge -sparse -f /t/sve-profraw-list -o /t/sve.profdata`
     Running `/sysroot/bin/llvm-cov export -format=text -instr-profile=/t/sve.profdata -object target/deps/a-1 -object target/deps/b-2 -ignore-filename-regex '{FILTER}' -summary-only`
"""
# llvm-objcopy keeps the coverage sections; this fake prefixes them to the copied object.
OBJCOPY = """#!/usr/bin/env python3
import sys
from pathlib import Path
*sections, source, output = sys.argv[1:]
Path(output).write_text(" ".join(sections) + "\\n" + Path(source).read_text())
"""
# The synthetic profile is a JSON line total; merging adds the covered lines.
PROFDATA = """#!/usr/bin/env python3
import json, sys
from pathlib import Path
arguments = sys.argv[1:]
output = Path(arguments[arguments.index("-o") + 1])
profiles = [json.loads(Path(a).read_text()) for a in arguments[2:arguments.index("-o")]]
covered = min(sum(p["covered"] for p in profiles), profiles[0]["count"])
output.write_text(json.dumps({"count": profiles[0]["count"], "covered": covered}))
"""
COV = """#!/usr/bin/env python3
import json, sys
from pathlib import Path
arguments = sys.argv[1:]
assert arguments[0] == "export"
assert arguments[arguments.index("-ignore-filename-regex") + 1] == FILTER
profile = json.loads(Path(next(a for a in arguments if a.startswith("-instr-profile="))[15:]).read_text())
if "-format=lcov" in arguments:
    print("SF:/w/src/lib.rs\\nend_of_record")
else:
    profile["percent"] = 100 * profile["covered"] / profile["count"]
    print(json.dumps({"data": [{"totals": {"lines": profile}}]}))
""".replace("FILTER", repr(FILTER), 1)


def summary(count: int, covered: int) -> str:
    """Render an llvm-cov export summary."""
    lines = {"count": count, "covered": covered, "percent": 100 * covered / count}
    return json.dumps({"data": [{"totals": {"lines": lines}}]})


class RustCoverageTests(unittest.TestCase):
    """The merged gate must be llvm-cov's own total over identical shard inputs."""

    @override
    def setUp(self) -> None:
        """Create synthetic tools, built objects and a download folder."""
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.tools = self.root / "tools"
        self.tools.mkdir()
        for name, script in (
            ("llvm-objcopy", OBJCOPY),
            ("llvm-profdata", PROFDATA),
            ("llvm-cov", COV),
        ):
            (self.tools / name).write_text(script, encoding="utf-8")
            (self.tools / name).chmod(0o700)
        (self.root / "target/deps").mkdir(parents=True)
        for name in ("a-1", "b-2"):
            (self.root / "target/deps" / name).write_text(name, encoding="utf-8")
        self.folder = self.root / "download"

    def shard(self, index: int, covered: int, *, log: str = LOG) -> Path:
        """Package one shard as rust_tests.py does after its tests pass."""
        profile = self.root / f"profile-{index}"
        profile.write_text(json.dumps({"count": 10, "covered": covered}))
        report = self.root / f"summary-{index}.json"
        report.write_text(summary(10, covered), encoding="utf-8")
        output = self.folder / f"shard-{index}"
        rust_coverage.package(
            log.replace("/sysroot/bin", str(self.tools)).replace(
                "/t/sve.profdata", str(profile)
            ),
            report,
            self.root,
            output,
        )
        return output

    def merge(self) -> tuple[int, str]:
        """Run the coverage job's merge on the download folder."""
        output = io.StringIO()
        with (
            patch.object(rust_coverage, "llvm_tools", return_value=self.tools),
            contextlib.redirect_stdout(output),
            contextlib.redirect_stderr(output),
        ):
            status = rust_coverage.main([str(self.folder)])
        return status, output.getvalue()

    def test_reads_the_exact_export_command(self) -> None:
        """The filter and objects come from the command cargo-llvm-cov ran."""
        profile, objects, ignored = rust_coverage.options(
            rust_coverage.export_command(LOG)
        )
        assert profile == Path("/t/sve.profdata")
        assert objects == [Path("target/deps/a-1"), Path("target/deps/b-2")]
        assert ignored == FILTER
        for log in ("", LOG + LOG.splitlines()[-1]):
            with self.assertRaises(ValueError):
                rust_coverage.export_command(log)
        with self.assertRaises(ValueError):
            rust_coverage.options(["llvm-cov", "export", "-instr-profile=x"])

    def test_package_keeps_coverage_sections_filter_and_totals(self) -> None:
        """A shard ships its profile, stripped objects, filter and own totals."""
        output = self.shard(1, 4)
        assert json.loads((output / "llvm-cov.json").read_text()) == {
            "ignore_filename_regex": FILTER,
            "objects": ["a-1", "b-2"],
            "lines": {"count": 10, "covered": 4, "percent": 40},
        }
        assert (output / "objects/a-1").read_text() == (
            "--only-section=__llvm_covfun --only-section=__llvm_covmap "
            "--only-section=__llvm_prf_names\na-1"
        )
        assert json.loads((output / "shard.profdata").read_text())["covered"] == 4

    def test_merged_profiles_are_gated_at_ninety_percent(self) -> None:
        """llvm-cov computes the merged total; below 90 percent fails."""
        self.shard(1, 4)
        self.shard(2, 4)
        status, output = self.merge()
        assert status == 1
        assert "Merged Rust line coverage: 80.00%" in output
        totals = json.loads((self.folder / "coverage.json").read_text())["data"][0]
        assert totals["totals"]["lines"] == {"count": 10, "covered": 8, "percent": 80}
        # Exactly 90 percent passes, as with cargo-llvm-cov --fail-under-lines 90.
        self.shard(3, 1)
        status, output = self.merge()
        assert status == 0, output
        assert "Merged Rust line coverage: 90.00%" in output
        assert (self.folder / "lcov.info").read_text().startswith("SF:")

    def test_mismatched_shards_fail(self) -> None:
        """Different builds, filters or unreproducible totals never produce a gate result."""
        self.shard(1, 9)
        second = self.shard(2, 9)
        (second / "objects/b-2").write_text("rebuilt", encoding="utf-8")
        status, output = self.merge()
        assert status == 1
        assert "built different test objects" in output
        assert not (self.folder / "coverage.json").exists()

        (second / "objects/b-2").write_text(
            (self.folder / "shard-1/objects/b-2").read_text(), encoding="utf-8"
        )
        meta = json.loads((second / "llvm-cov.json").read_text())
        meta["lines"]["covered"] = 8
        (second / "llvm-cov.json").write_text(json.dumps(meta), encoding="utf-8")
        status, output = self.merge()
        assert status == 1
        assert "does not reproduce the totals of shard-2" in output

        meta["lines"]["covered"] = 9
        meta["ignore_filename_regex"] = "other"
        (second / "llvm-cov.json").write_text(json.dumps(meta), encoding="utf-8")
        status, output = self.merge()
        assert status == 1
        assert "different llvm-cov filter" in output

    def test_missing_shards_and_usage(self) -> None:
        """An empty download or a wrong call fails."""
        self.folder.mkdir()
        status, output = self.merge()
        assert status == 1
        assert "No Rust shard coverage" in output
        with contextlib.redirect_stderr(io.StringIO()):
            assert rust_coverage.main([]) == 2
