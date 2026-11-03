"""Generate ephemeral public card inputs before every complete Rust test scope.

Outside CI, inputs generated from an identical working tree are reused: the
generator's whole input (see INPUTS) is hashed before and after generation, and a
copy is kept under the user cache only when both hashes agree.

`--shard=I/N --shard-report=DIR` runs one CI slice: every test, chosen by a hash of
its name, belongs to exactly one shard. The slice's coverage inputs go to DIR/coverage
(see rust_coverage.py) and its counts to DIR/shard.json, so the coverage job can merge
the slices and check that together they ran every listed test.
"""

import fcntl
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- fixed tool argv; no shell
import sys
import tempfile
from collections.abc import Generator
from contextlib import contextmanager
from pathlib import Path

import rust_coverage

ROOT = Path(__file__).resolve().parents[2]
DEFAULT = [
    "llvm-cov",
    "--locked",
    "--workspace",
    "--summary-only",
    "--fail-under-lines",
    str(rust_coverage.LINE_THRESHOLD),
]
# The shards run the same workspace scope as DEFAULT; the 90% gate applies to their merged coverage.
SHARD_SCOPE = ["--locked", "--workspace"]
SHARD = re.compile(r"--shard=([1-9][0-9]*)/([1-9][0-9]*)")
LISTED = re.compile(r"^(.+): test$")
# Everything tests.support.repository_current reads; renaming the generator or its
# outputs must bump GENERATOR.
INPUTS = (
    "card-data",
    "authored",
    "dsl",
    "tests/engine/card-input.json",
    "carddb/src",
    "carddb/tests/__init__.py",
    "carddb/tests/support",
    "carddb/pyproject.toml",
    "carddb/uv.lock",
)
GENERATOR = b"repository_current cards.jsonl selection.json v2"
OUTPUTS = ("cards.jsonl", "selection.json")
KEPT_INPUTS = 8


def executable(name: str) -> str:
    """Resolve an installed tool without shell interpolation."""
    path = shutil.which(name)
    if path is None:
        raise FileNotFoundError("Required test tool is unavailable")
    return path


def shard_of(name: str, total: int) -> int:
    """Stable across processes and machines; a name appearing in two binaries stays in one shard."""
    digest = hashlib.sha256(name.encode()).digest()
    return int.from_bytes(digest[:8], "big") % total + 1


def input_digest(root: Path) -> str:
    """Hash directory topology and regular file bytes; missing or unsafe inputs fail."""
    digest = hashlib.sha256(GENERATOR)
    for name in INPUTS:
        path = root / name
        for parent in path.parents:
            if parent == root:
                break
            if parent.is_symlink():
                raise ValueError(f"Symlink engine input: {name}")
        pending = [path]
        while pending:
            item = pending.pop()
            if item.name == "__pycache__":
                continue
            mode = item.lstat().st_mode
            if not (stat.S_ISDIR(mode) or stat.S_ISREG(mode)):
                raise ValueError(f"Nonregular engine input: {item.relative_to(root)}")
            relative = item.relative_to(root).as_posix().encode()
            digest.update(b"d" if stat.S_ISDIR(mode) else b"f")
            digest.update(len(relative).to_bytes(8, "big") + relative)
            if stat.S_ISDIR(mode):
                # iterdir propagates unreadable-directory errors instead of hiding them.
                pending.extend(sorted(item.iterdir(), reverse=True))
            else:
                content = item.read_bytes()
                digest.update(len(content).to_bytes(8, "big") + content)
    return digest.hexdigest()


def cache_root() -> Path | None:
    """Local reuse only: CI always generates into a fresh temporary folder."""
    if os.environ.get("CI"):
        return None
    base = os.environ.get("XDG_CACHE_HOME") or str(Path.home() / ".cache")
    return Path(base) / "sve-kit" / "engine-inputs"


@contextmanager
def _cache_access(cache: Path) -> Generator[None]:
    cache.mkdir(parents=True, exist_ok=True)
    with (cache / ".lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)


def reuse(cache: Path, digest: str, folder: Path) -> bool:
    """Copy a complete cached pair into the private temporary folder."""
    if not cache.is_dir():
        return False
    # A publisher must not prune this pair between its two copies.
    with _cache_access(cache):
        entry = cache / digest
        if not all((entry / name).is_file() for name in OUTPUTS):
            return False
        for name in OUTPUTS:
            shutil.copyfile(entry / name, folder / name)
        entry.touch()
    return True


def keep(cache: Path, digest: str, folder: Path) -> None:
    """Publish a generated pair atomically and keep only the newest entries."""
    cache.mkdir(parents=True, exist_ok=True)
    entry = cache / digest
    staging = Path(tempfile.mkdtemp(prefix=f".{digest}-", dir=cache))
    try:
        for name in OUTPUTS:
            shutil.copyfile(folder / name, staging / name)
        with _cache_access(cache):
            if entry.exists() and not all((entry / name).is_file() for name in OUTPUTS):
                shutil.rmtree(entry)
            if not entry.exists():
                staging.rename(entry)
            entry.touch()
            entries = sorted(
                (
                    path
                    for path in cache.iterdir()
                    if path.is_dir() and not path.name.startswith(".")
                ),
                key=lambda path: path.stat().st_mtime,
                reverse=True,
            )
            for old in entries[KEPT_INPUTS:]:
                shutil.rmtree(old, ignore_errors=True)
    finally:
        shutil.rmtree(staging, ignore_errors=True)


def generate(output: Path, selection: Path) -> subprocess.Popen[bytes]:
    """Start writing the public card inputs; Cargo can build meanwhile."""
    return subprocess.Popen(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed local test adapter
        [
            executable("uv"),
            "run",
            "--directory",
            str(ROOT / "carddb"),
            "--locked",
            "--no-default-groups",
            "python",
            "-m",
            "tests.support.repository_current",
            str(output),
            str(selection),
        ]
    )


def cargo(
    arguments: list[str],
    *,
    env: dict[str, str] | None = None,
    capture: bool = False,
    capture_log: bool = False,
) -> subprocess.CompletedProcess[str]:
    """Run Cargo through mise with explicit argument boundaries."""
    return subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- Cargo receives explicit argument boundaries
        [executable("mise"), "exec", "--", "cargo", *arguments],
        cwd=ROOT,
        env=env,
        check=False,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture_log else None,
        text=True,
    )


def list_tests() -> list[str] | int:
    """Build every test binary with coverage and list its tests (or return Cargo's failure status)."""
    cleaned = cargo(["llvm-cov", "clean", "--workspace"])
    if cleaned.returncode:
        return cleaned.returncode
    listing = cargo(
        ["llvm-cov", "--no-report", *SHARD_SCOPE, "--", "--list", "--format=terse"],
        capture=True,
    )
    if listing.returncode:
        return listing.returncode
    return [
        match[1]
        for line in listing.stdout.splitlines()
        if (match := LISTED.match(line))
    ]


def run_shard(
    names: list[str], shard: tuple[int, int, Path], environment: dict[str, str]
) -> int:
    """Run and report only this shard's tests from the binaries list_tests built."""
    index, total, report = shard
    selected = [name for name in names if shard_of(name, total) == index]
    if not selected:
        # Without a filter libtest would run every test.
        sys.stderr.write("This shard selected no tests; use fewer shards\n")
        return 1
    # --no-report runs never clean, so this reuses the binaries the listing built.
    tests = cargo(
        [
            "llvm-cov",
            "--no-report",
            *SHARD_SCOPE,
            "--",
            "--exact",
            *selected,
        ],
        env=environment,
    )
    if tests.returncode:
        return tests.returncode
    report.mkdir(parents=True, exist_ok=True)
    summary = report / "summary.json"
    # -vv prints the exact llvm-cov command, which the coverage job repeats on the merged profiles.
    exported = cargo(
        [
            "llvm-cov",
            "report",
            "--json",
            "--summary-only",
            "--output-path",
            str(summary),
            "-vv",
        ],
        capture_log=True,
    )
    sys.stderr.write(exported.stderr)
    if exported.returncode:
        return exported.returncode
    rust_coverage.package(exported.stderr, summary, ROOT, report / "coverage")
    (report / "shard.json").write_text(
        json.dumps(
            {
                "index": index,
                "total": total,
                "collected": len(names),
                "selected": len(selected),
            }
        ),
        encoding="utf-8",
    )
    return 0


def parse_shard(arguments: list[str]) -> tuple[int, int, Path] | None:
    """Return the 1-based shard, shard count and report folder, or None for a plain Cargo run."""
    match arguments:
        case [first, *_] if not first.startswith("--shard"):
            return None
        case []:
            return None
        case [shard_argument, report_argument] if (
            (shard := SHARD.fullmatch(shard_argument))
            and report_argument.startswith("--shard-report=")
            and int(shard[1]) <= int(shard[2])
        ):
            return (
                int(shard[1]),
                int(shard[2]),
                Path(report_argument.removeprefix("--shard-report=")),
            )
        case _:
            msg = "Usage: rust_tests.py --shard=I/N --shard-report=DIR (1 <= I <= N)"
            raise ValueError(msg)


def main(arguments: list[str]) -> int:
    """Never reuse a caller's private snapshot or inputs from another working tree."""
    try:
        shard = parse_shard(arguments)
    except ValueError as error:
        sys.stderr.write(f"{error}\n")
        return 2
    with tempfile.TemporaryDirectory(prefix="sve-engine-input-") as temporary:
        folder = Path(temporary)
        output = folder / OUTPUTS[0]
        selection = folder / OUTPUTS[1]
        cache = cache_root()
        digest = None if cache is None else input_digest(ROOT)
        generation = (
            None
            if cache is not None and digest is not None and reuse(cache, digest, folder)
            else generate(output, selection)
        )
        try:
            names = list_tests() if shard is not None else []
        finally:
            # Never remove the folder while the generator still writes into it.
            if generation is not None:
                generation.wait()
        if generation is not None:
            if generation.returncode:
                raise subprocess.CalledProcessError(
                    generation.returncode, generation.args
                )
            # Inputs edited during generation would make the pair stale for this digest.
            if cache is not None and digest == input_digest(ROOT):
                keep(cache, digest, folder)
        environment = {
            **os.environ,
            "SVE_TEST_SNAPSHOT": str(output),
            "SVE_TEST_CURRENT_SELECTION": str(selection),
        }
        if shard is None:
            return cargo(arguments or DEFAULT, env=environment).returncode
        if isinstance(names, int):
            return names
        return run_shard(names, shard, environment)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
