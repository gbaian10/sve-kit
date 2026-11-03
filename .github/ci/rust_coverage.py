"""Hand one Rust test shard's coverage to the coverage job, and gate the merged shards there.

LLVM's line total adds up every function's lines, so a line shared by two functions counts
twice; it cannot be rebuilt from per-line reports. Each shard therefore ships its merged
profile, the coverage sections of the objects it ran, and the exact `llvm-cov export`
filter that cargo-llvm-cov used. The coverage job merges the profiles and lets llvm-cov
compute the same report a single run would. It first checks that every shard built the same
objects and that llvm-cov reproduces each shard's own cargo-llvm-cov totals from the
shipped inputs, so a mismatch fails instead of changing the measurement.
"""

import hashlib
import json
import shlex
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- fixed LLVM tool argv; no shell
import sys
from pathlib import Path

# cargo-llvm-cov --fail-under-lines fails when the percentage is below this.
LINE_THRESHOLD = 90
# Everything llvm-cov reads from an object; the rest (code, debug info) is dropped.
SECTIONS = ("__llvm_covfun", "__llvm_covmap", "__llvm_prf_names")
EXPORT = "llvm-cov export "


def export_command(log: str) -> list[str]:
    """Return the `llvm-cov export` argv that `cargo llvm-cov report -vv` ran."""
    commands = [
        shlex.split(line.split("`", 1)[1].rsplit("`", 1)[0])
        for line in log.splitlines()
        if line.lstrip().startswith("Running `") and EXPORT in line
    ]
    if len(commands) != 1:
        msg = "Expected exactly one llvm-cov export command in the cargo-llvm-cov log"
        raise ValueError(msg)
    return commands[0]


def options(command: list[str]) -> tuple[Path, list[Path], str]:
    """Return the profile, objects and filename filter of an export command."""
    profiles = [
        Path(argument.removeprefix("-instr-profile="))
        for argument in command
        if argument.startswith("-instr-profile=")
    ]
    objects = [
        Path(command[index + 1])
        for index, argument in enumerate(command)
        if argument == "-object"
    ]
    filters = [
        command[index + 1]
        for index, argument in enumerate(command)
        if argument == "-ignore-filename-regex"
    ]
    if len(profiles) != 1 or not objects or len(filters) != 1:
        msg = "Unexpected llvm-cov export arguments"
        raise ValueError(msg)
    return profiles[0], objects, filters[0]


def line_totals(summary: str) -> dict[str, float]:
    """Return llvm-cov's own line count, covered lines and percentage from an export summary."""
    data = json.loads(summary)["data"]
    if not isinstance(data, list) or len(data) != 1:
        msg = "Expected one llvm-cov export"
        raise ValueError(msg)
    totals = data[0]["totals"]["lines"]
    return {key: float(totals[key]) for key in ("count", "covered", "percent")}


def package(log: str, summary: Path, root: Path, output: Path) -> None:
    """Keep this shard's profile, object coverage sections, filter and own totals."""
    command = export_command(log)
    profile, relative, ignored = options(command)
    # cargo-llvm-cov runs llvm-cov from the workspace root.
    objects = [root / path for path in relative]
    tools = Path(command[0]).parent
    (output / "objects").mkdir(parents=True)
    shutil.copyfile(profile, output / "shard.profdata")
    for path in objects:
        subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed LLVM tool argv
            [
                str(tools / "llvm-objcopy"),
                *(f"--only-section={section}" for section in SECTIONS),
                str(path),
                str(output / "objects" / path.name),
            ],
            check=True,
        )
    (output / "llvm-cov.json").write_text(
        json.dumps(
            {
                "ignore_filename_regex": ignored,
                "objects": [path.name for path in objects],
                "lines": line_totals(summary.read_text(encoding="utf-8")),
            }
        ),
        encoding="utf-8",
    )


def digest(path: Path) -> str:
    """Hash one stripped object."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def llvm_tools() -> Path:
    """Locate the pinned toolchain's LLVM tools (the llvm-tools component)."""
    rustc = shutil.which("rustc")
    if rustc is None:
        raise FileNotFoundError("rustc is unavailable")
    sysroot = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed rustc query
        [rustc, "--print", "sysroot"], capture_output=True, text=True, check=True
    ).stdout.strip()
    host = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed rustc query
        [rustc, "--print", "host-tuple"], capture_output=True, text=True, check=True
    ).stdout.strip()
    return Path(sysroot) / "lib/rustlib" / host / "bin"


def export(
    tools: Path, profile: Path, objects: list[Path], ignored: str, *formats: str
) -> str:
    """Run llvm-cov export like cargo-llvm-cov does."""
    return subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed LLVM tool argv
        [
            str(tools / "llvm-cov"),
            "export",
            *formats,
            f"-instr-profile={profile}",
            *(argument for path in objects for argument in ("-object", str(path))),
            "-ignore-filename-regex",
            ignored,
        ],
        capture_output=True,
        text=True,
        check=True,
    ).stdout


def merge(folder: Path, tools: Path) -> float:
    """Write coverage.json and lcov.info for the merged shards and return the line percentage."""
    shards = sorted(path.parent for path in folder.glob("shard-*/llvm-cov.json"))
    if not shards:
        msg = "No Rust shard coverage"
        raise ValueError(msg)
    metadata = [
        json.loads((shard / "llvm-cov.json").read_text(encoding="utf-8"))
        for shard in shards
    ]
    ignored = metadata[0]["ignore_filename_regex"]
    names = metadata[0]["objects"]
    built = {name: digest(shards[0] / "objects" / name) for name in names}
    for shard, meta in zip(shards, metadata, strict=True):
        if meta["ignore_filename_regex"] != ignored or meta["objects"] != names:
            msg = f"{shard.name} used a different llvm-cov filter or object list"
            raise ValueError(msg)
        if any(digest(shard / "objects" / name) != built[name] for name in names):
            msg = f"{shard.name} built different test objects"
            raise ValueError(msg)
    objects = [shards[0] / "objects" / name for name in names]
    for shard, meta in zip(shards, metadata, strict=True):
        summary = export(
            tools,
            shard / "shard.profdata",
            objects,
            ignored,
            "-format=text",
            "-summary-only",
        )
        totals = line_totals(summary)
        if any(totals[key] != meta["lines"][key] for key in ("count", "covered")):
            msg = f"llvm-cov does not reproduce the totals of {shard.name}"
            raise ValueError(msg)
    merged = folder / "merged.profdata"
    subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed LLVM tool argv
        [
            str(tools / "llvm-profdata"),
            "merge",
            "-sparse",
            *(str(shard / "shard.profdata") for shard in shards),
            "-o",
            str(merged),
        ],
        check=True,
    )
    summary = export(tools, merged, objects, ignored, "-format=text", "-summary-only")
    (folder / "coverage.json").write_text(summary, encoding="utf-8")
    (folder / "lcov.info").write_text(
        export(tools, merged, objects, ignored, "-format=lcov"), encoding="utf-8"
    )
    return line_totals(summary)["percent"]


def main(arguments: list[str]) -> int:
    """Merge the downloaded shards in a folder and apply the line gate."""
    if len(arguments) != 1:
        sys.stderr.write("Usage: rust_coverage.py SHARD_COVERAGE_FOLDER\n")
        return 2
    try:
        percent = merge(Path(arguments[0]), llvm_tools())
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        subprocess.CalledProcessError,
    ) as error:
        sys.stderr.write(f"Rust coverage merge failed: {error}\n")
        return 1
    sys.stdout.write(f"Merged Rust line coverage: {percent:.2f}%\n")
    return int(percent < LINE_THRESHOLD)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
