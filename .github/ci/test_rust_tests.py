# ruff: file-ignore[no-self-use, magic-value-comparison, pytest-unittest-raises-assertion] -- standalone unittest (no pytest) with synthetic exit statuses
"""Exercise the public-input wrapper without compiling or reading official data."""

import contextlib
import io
import json
import os
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- synthetic process results
import tempfile
import unittest
from collections.abc import Callable
from concurrent.futures import Future, ThreadPoolExecutor
from pathlib import Path
from typing import override
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
        during_generation: Callable[[], object] | None = None,
    ) -> None:
        self.generated: list[Path] = []
        self.packaged: list[Path] = []
        self.cargo: list[list[str]] = []
        self.environments: list[dict[str, str] | None] = []
        self.generation_status = generation_status
        self.test_status = test_status
        self.listing = listing
        self.during_generation = during_generation
        self.inputs: list[str] = []
        self.generated_folders: set[Path] = set()

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
                if tools.during_generation is not None:
                    tools.during_generation()
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
        inputs = [
            Path(environment["SVE_TEST_SNAPSHOT"]),
            Path(environment["SVE_TEST_CURRENT_SELECTION"]),
        ]
        if self.generated:
            assert inputs == self.generated[-2:]
        assert inputs[0].parent == inputs[1].parent
        self.inputs = [path.read_text(encoding="utf-8") for path in inputs]
        self.generated_folders.add(inputs[0].parent)
        return subprocess.CompletedProcess(argv, self.test_status)


def run(
    arguments: list[str],
    tools: FakeTools,
    *,
    root: Path | None = None,
    cache: Path | None = None,
) -> int:
    """Run the wrapper with fake tools and an inherited private snapshot path."""

    def package(log: str, summary: Path, root_path: Path, output: Path) -> None:
        assert log == "Running `llvm-cov export`"
        assert summary.exists()
        assert root_path == rust_tests.ROOT
        output.mkdir()
        tools.packaged.append(output)

    with (
        patch.object(rust_tests, "executable", side_effect=lambda name: name),
        patch.object(rust_tests, "ROOT", root or rust_tests.ROOT),
        patch.object(rust_coverage, "package", side_effect=package),
        patch.object(subprocess, "Popen", side_effect=tools.popen),
        patch.object(subprocess, "run", side_effect=tools.run),
        patch.dict(
            os.environ,
            {
                "SVE_TEST_SNAPSHOT": "/unused/inherited-snapshot",
                "SVE_TEST_CURRENT_SELECTION": "/unused/inherited-selection",
                "CI": "true" if cache is None else "",
                "XDG_CACHE_HOME": (
                    os.environ.get("XDG_CACHE_HOME", "")
                    if cache is None
                    else str(cache)
                ),
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


def synthetic_tree(root: Path) -> None:
    """Every declared generator input, as tiny synthetic files."""
    for name in rust_tests.INPUTS:
        path = root / name
        if Path(name).suffix:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(name, encoding="utf-8")
        else:
            (path / "nested").mkdir(parents=True)
            (path / "nested" / "data.txt").write_text(name, encoding="utf-8")
            (path / "__pycache__").mkdir()
            (path / "__pycache__" / "ignored.pyc").write_bytes(b"\x00")


class InputReuse(unittest.TestCase):
    @override
    def setUp(self) -> None:
        """Each case starts from its own synthetic tree and empty cache."""
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "repo"
        self.cache_home = Path(temporary.name) / "cache"
        self.cache = self.cache_home / "sve-kit/engine-inputs"
        synthetic_tree(self.root)

    def run_tests(self, tools: FakeTools, *, cache: Path | None = None) -> int:
        """Run a plain test scope against the synthetic tree."""
        return run([], tools, root=self.root, cache=cache or self.cache_home)

    def test_identical_tree_reuses_without_generating(self) -> None:
        """A second run on unchanged inputs copies the kept pair instead of generating."""
        first = FakeTools()
        assert self.run_tests(first) == 0
        assert len(first.generated) == 2
        kept = [path for path in self.cache.iterdir() if not path.name.startswith(".")]
        assert len(kept) == 1
        second = FakeTools()
        assert self.run_tests(second) == 0
        assert second.generated == []
        assert second.inputs == ["synthetic", "synthetic"]
        # The private copy is still removed; the kept pair stays.
        assert not any(folder.exists() for folder in second.generated_folders)
        assert (kept[0] / "cards.jsonl").is_file()

    def test_changed_input_regenerates(self) -> None:
        """Any edited input file, including nested ones, misses the kept pair."""
        assert self.run_tests(FakeTools()) == 0
        (self.root / "authored" / "nested" / "data.txt").write_text(
            "edited", encoding="utf-8"
        )
        tools = FakeTools()
        assert self.run_tests(tools) == 0
        assert len(tools.generated) == 2

    def test_empty_directory_change_regenerates(self) -> None:
        """Directory topology checked by the generator is part of the input."""
        assert self.run_tests(FakeTools()) == 0
        (self.root / "authored/products/unexpected").mkdir(parents=True)
        tools = FakeTools()
        assert self.run_tests(tools) == 0
        assert len(tools.generated) == 2

    def test_symlink_with_identical_content_cannot_reuse(self) -> None:
        """A cached pair cannot bypass the data readers' symlink rejection."""
        assert self.run_tests(FakeTools()) == 0
        target = self.root / "card-data/nested/data.txt"
        external = self.root.parent / "external.txt"
        external.write_bytes(target.read_bytes())
        target.unlink()
        target.symlink_to(external)
        tools = FakeTools()
        with self.assertRaises(ValueError):
            self.run_tests(tools)
        assert tools.cargo == []
        assert tools.generated == []

    def test_bytecode_does_not_change_the_digest(self) -> None:
        """Interpreter caches are not inputs."""
        before = rust_tests.input_digest(self.root)
        (self.root / "carddb/src/__pycache__/ignored.pyc").write_bytes(b"\x01")
        assert rust_tests.input_digest(self.root) == before

    def test_version_file_written_by_install_is_not_an_input(self) -> None:
        """The first install writes the version file during generation; the pair is still kept."""
        version = self.root / "carddb/src/sve_carddb/_version.py"
        version.parent.mkdir(parents=True, exist_ok=True)
        first = FakeTools(
            during_generation=lambda: version.write_text("v1", encoding="utf-8")
        )
        assert self.run_tests(first) == 0
        assert len(first.generated) == 2
        version.write_text("v2", encoding="utf-8")
        second = FakeTools()
        assert self.run_tests(second) == 0
        assert second.generated == []

    def test_missing_input_fails_before_cargo(self) -> None:
        """A missing input never falls back to generation or a kept pair."""
        (self.root / "tests/engine/card-input.json").unlink()
        tools = FakeTools()
        with self.assertRaises(FileNotFoundError):
            self.run_tests(tools)
        assert tools.cargo == []
        assert tools.generated == []

    def test_inputs_edited_during_generation_are_not_kept(self) -> None:
        """The pair may not match either version of the edited tree."""
        target = self.root / "card-data" / "nested" / "data.txt"
        tools = FakeTools(
            during_generation=lambda: target.write_text("edited", encoding="utf-8")
        )
        assert self.run_tests(tools) == 0
        assert not self.cache.exists() or not any(
            not path.name.startswith(".") for path in self.cache.iterdir()
        )

    def test_failed_generation_keeps_nothing(self) -> None:
        """Only a successful generation is kept."""
        with self.assertRaises(subprocess.CalledProcessError):
            self.run_tests(FakeTools(generation_status=3))
        assert not self.cache.exists()

    def test_incomplete_kept_pair_regenerates(self) -> None:
        """A kept folder missing either output is ignored."""
        assert self.run_tests(FakeTools()) == 0
        kept = next(
            path for path in self.cache.iterdir() if not path.name.startswith(".")
        )
        (kept / "selection.json").unlink()
        tools = FakeTools()
        assert self.run_tests(tools) == 0
        assert len(tools.generated) == 2

    def test_cleanup_during_reuse_does_not_break_tests(self) -> None:
        """Another publisher may prune old entries while a test copies its pair."""
        assert self.run_tests(FakeTools()) == 0
        digest = rust_tests.input_digest(self.root)
        entry = self.cache / digest
        source = self.root.parent / "pair"
        source.mkdir()
        for name in ("cards.jsonl", "selection.json"):
            (source / name).write_text("synthetic", encoding="utf-8")
        os.utime(entry, (1, 1))
        for index in range(7):
            rust_tests.keep(self.cache, f"{index:064x}", source)
        original_copy = shutil.copyfile
        pruning: Future[None] | None = None
        with ThreadPoolExecutor(max_workers=1) as executor:

            def copy_and_prune(src: Path, dst: Path) -> str:
                nonlocal pruning
                result = original_copy(src, dst)
                if src == entry / "cards.jsonl":
                    pruning = executor.submit(
                        rust_tests.keep, self.cache, "f" * 64, source
                    )
                    with contextlib.suppress(TimeoutError):
                        pruning.result(timeout=1)
                return str(result)

            tools = FakeTools()
            with patch.object(shutil, "copyfile", side_effect=copy_and_prune):
                assert self.run_tests(tools) == 0
            assert pruning is not None
            pruning.result(timeout=5)
        assert tools.generated == []
        assert tools.inputs == ["synthetic", "synthetic"]

    def test_simultaneous_publishers_keep_one_complete_pair(self) -> None:
        """Two worktrees with identical inputs can both publish successfully."""
        source = self.root.parent / "pair"
        source.mkdir()
        (source / "cards.jsonl").write_text("cards", encoding="utf-8")
        (source / "selection.json").write_text("selection", encoding="utf-8")
        digest = rust_tests.input_digest(self.root)
        with ThreadPoolExecutor(max_workers=2) as executor:
            first = executor.submit(rust_tests.keep, self.cache, digest, source)
            second = executor.submit(rust_tests.keep, self.cache, digest, source)
            first.result(timeout=5)
            second.result(timeout=5)
        destination = self.root.parent / "private"
        destination.mkdir()
        assert rust_tests.reuse(self.cache, digest, destination)
        assert (destination / "cards.jsonl").read_text(encoding="utf-8") == "cards"
        assert (destination / "selection.json").read_text(
            encoding="utf-8"
        ) == "selection"

    def test_only_the_newest_pairs_are_kept(self) -> None:
        """Old working trees do not accumulate kept pairs."""
        for index in range(rust_tests.KEPT_INPUTS + 2):
            (self.root / "dsl" / "nested" / "data.txt").write_text(
                str(index), encoding="utf-8"
            )
            assert self.run_tests(FakeTools()) == 0
        kept = [path for path in self.cache.iterdir() if not path.name.startswith(".")]
        assert len(kept) == rust_tests.KEPT_INPUTS

    def test_ci_never_reuses(self) -> None:
        """CI keeps generating into a fresh temporary folder, never the user cache."""
        with patch.dict(os.environ, {"CI": "true"}):
            assert rust_tests.cache_root() is None
        with patch.dict(os.environ, {"XDG_CACHE_HOME": str(self.cache)}):
            os.environ.pop("CI", None)
            assert rust_tests.cache_root() == self.cache / "sve-kit" / "engine-inputs"

    def test_ci_main_generates_despite_an_existing_local_pair(self) -> None:
        """The real CI policy bypasses even a complete, matching local cache."""
        assert self.run_tests(FakeTools()) == 0
        tools = FakeTools()
        with patch.dict(os.environ, {"XDG_CACHE_HOME": str(self.cache_home)}):
            assert run([], tools, root=self.root) == 0
        assert len(tools.generated) == 2
