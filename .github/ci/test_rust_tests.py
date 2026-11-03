# ruff: file-ignore[no-self-use, magic-value-comparison, pytest-unittest-raises-assertion] -- standalone unittest (no pytest) with synthetic exit statuses
"""Exercise the public-input wrapper without compiling or reading official data."""

import contextlib
import io
import json
import os
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- synthetic process results
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import rust_coverage
import rust_tests

LISTING = """\
boundaries::accepts_input: test
m0::case_0: test
current_repository::runtime::bp01::plays: test
current_repository::runtime::bp01::plays_twice: test
src/lib.rs - catalog::Catalog (line 12): test
shared::every_scenario: test
shared::every_scenario: test
5 tests, 0 benchmarks
not a listed test
"""
NAMES = [
    "boundaries::accepts_input",
    "m0::case_0",
    "current_repository::runtime::bp01::plays",
    "current_repository::runtime::bp01::plays_twice",
    "src/lib.rs - catalog::Catalog (line 12)",
    "shared::every_scenario",
    "shared::every_scenario",
]


class FakeTools:
    """Record every tool call; the generator writes both inputs when waited for."""

    def __init__(
        self,
        *,
        generation_status: int = 0,
        test_status: int = 0,
        listing: str = LISTING,
    ) -> None:
        self.generated: list[Path] = []
        self.packaged: list[Path] = []
        self.cargo: list[list[str]] = []
        self.environments: list[dict[str, str] | None] = []
        self.generation_status = generation_status
        self.test_status = test_status
        self.listing = listing

    def popen(self, argv: list[str], **_: object) -> subprocess.Popen[bytes]:
        """Start a fake generator that writes its two outputs on wait."""
        assert argv[0] == "uv"
        outputs = [Path(filename) for filename in argv[-2:]]
        assert all(output.is_absolute() and not output.exists() for output in outputs)
        tools = self

        class Generation:
            args = argv
            returncode: int | None = None

            def wait(self) -> int:
                for output in outputs:
                    output.write_text("synthetic", encoding="utf-8")
                tools.generated.extend(outputs)
                self.returncode = tools.generation_status
                return self.returncode

        return Generation()  # type: ignore[return-value]

    def run(
        self, argv: list[str], **kwargs: object
    ) -> subprocess.CompletedProcess[str]:
        """Answer one Cargo call; tests may only run once both inputs exist."""
        assert argv[:4] == ["mise", "exec", "--", "cargo"]
        arguments = argv[4:]
        self.cargo.append(arguments)
        environment = kwargs.get("env")
        self.environments.append(environment)  # type: ignore[arg-type]
        if "--list" in arguments:
            return subprocess.CompletedProcess(argv, 0, stdout=self.listing)
        if "report" in arguments:
            assert kwargs["stderr"] == subprocess.PIPE
            summary = Path(arguments[arguments.index("--output-path") + 1])
            summary.write_text("{}", encoding="utf-8")
            return subprocess.CompletedProcess(
                argv, 0, stderr="Running `llvm-cov export`"
            )
        if arguments[:2] == ["llvm-cov", "clean"]:
            return subprocess.CompletedProcess(argv, 0)
        assert isinstance(environment, dict)
        assert Path(environment["SVE_TEST_SNAPSHOT"]) == self.generated[0]
        assert Path(environment["SVE_TEST_CURRENT_SELECTION"]) == self.generated[1]
        assert all(path.exists() for path in self.generated)
        return subprocess.CompletedProcess(argv, self.test_status)


def run(arguments: list[str], tools: FakeTools) -> int:
    """Run the wrapper with fake tools and an inherited private snapshot path."""

    def package(log: str, summary: Path, root: Path, output: Path) -> None:
        assert log == "Running `llvm-cov export`"
        assert summary.exists()
        assert root == rust_tests.ROOT
        output.mkdir()
        tools.packaged.append(output)

    with (
        patch.object(rust_tests, "executable", side_effect=lambda name: name),
        patch.object(rust_coverage, "package", side_effect=package),
        patch.object(subprocess, "Popen", side_effect=tools.popen),
        patch.object(subprocess, "run", side_effect=tools.run),
        patch.dict(
            os.environ,
            {
                "SVE_TEST_SNAPSHOT": "/unused/inherited-snapshot",
                "SVE_TEST_CURRENT_SELECTION": "/unused/inherited-selection",
            },
        ),
    ):
        return rust_tests.main(arguments)


class RustTests(unittest.TestCase):
    def test_regenerates_and_cleans_input(self) -> None:
        """An inherited snapshot path cannot skip generation or cleanup."""
        for arguments in (
            [],
            ["mutants", "--workspace", "-j", "4", "--timeout", "120"],
        ):
            tools = FakeTools(test_status=7)
            assert run(arguments, tools) == 7
            command = next(call for call in tools.cargo if "--workspace" in call)
            if arguments:
                assert command == arguments
            else:
                assert command[0] == "llvm-cov"
                assert "--locked" in command
                assert command[command.index("--fail-under-lines") + 1] == "90"
                assert "--test" not in command
            assert not any(path.exists() for path in tools.generated)

    def test_failed_generation_never_runs_cargo(self) -> None:
        """Tests cannot run against a missing or partial input."""
        tools = FakeTools(generation_status=3)
        with self.assertRaises(subprocess.CalledProcessError):
            run([], tools)
        assert tools.cargo == []

    def test_shards_partition_every_listed_test(self) -> None:
        """Shards run disjoint name sets that together cover the whole listing."""
        with tempfile.TemporaryDirectory() as temporary:
            union: list[str] = []
            previous: set[str] = set()
            for index in (1, 2, 3):
                report = Path(temporary) / f"shard-{index}"
                tools = FakeTools()
                assert (
                    run([f"--shard={index}/3", f"--shard-report={report}"], tools) == 0
                )
                listing = next(call for call in tools.cargo if "--list" in call)
                tests = next(call for call in tools.cargo if "--exact" in call)
                for command in (listing, tests):
                    assert command[0] == "llvm-cov"
                    assert "--workspace" in command
                    assert "--locked" in command
                    assert "--test" not in command
                assert "--exact" in tests[tests.index("--") + 1 :]
                selected = tests[tests.index("--exact") + 1 :]
                assert selected
                assert not previous.intersection(selected)
                previous.update(selected)
                union.extend(selected)
                assert tools.packaged == [report / "coverage"]
                assert json.loads((report / "shard.json").read_text()) == {
                    "index": index,
                    "total": 3,
                    "collected": len(NAMES),
                    "selected": len(selected),
                }
                assert not any(path.exists() for path in tools.generated)
            assert sorted(union) == sorted(NAMES)

    def test_failing_shard_writes_no_report(self) -> None:
        """A failed slice cannot contribute coverage or counts."""
        with tempfile.TemporaryDirectory() as temporary:
            report = Path(temporary) / "report"
            tools = FakeTools(test_status=101)
            assert run(["--shard=1/1", f"--shard-report={report}"], tools) == 101
            assert not report.exists()

    def test_empty_shard_fails_instead_of_running_everything(self) -> None:
        """Without a filter libtest runs every test, so an empty slice is an error."""
        with tempfile.TemporaryDirectory() as temporary:
            report = Path(temporary) / "report"
            tools = FakeTools(listing="0 tests, 0 benchmarks\n")
            with contextlib.redirect_stderr(io.StringIO()):
                assert run(["--shard=1/3", f"--shard-report={report}"], tools) == 1
            assert not any("--exact" in call for call in tools.cargo)
            assert not report.exists()

    def test_invalid_shard_arguments(self) -> None:
        """A malformed shard request never falls back to a partial or plain run."""
        for arguments in (
            ["--shard=4/3", "--shard-report=x"],
            ["--shard=0/3", "--shard-report=x"],
            ["--shard=1/3"],
            ["--shard=1/3", "x"],
            ["--shard=1/3", "--shard-report=x", "extra"],
        ):
            tools = FakeTools()
            with contextlib.redirect_stderr(io.StringIO()):
                assert run(arguments, tools) == 2
            assert tools.cargo == []
            assert tools.generated == []
