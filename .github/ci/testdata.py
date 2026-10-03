"""Select explicit test scopes without treating missing credentials as a fork."""

import hashlib
import json
import os
import re
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- fixed local Git/Cargo commands
import sys
import tomllib
from pathlib import Path, PurePosixPath
from typing import cast

ROOT = Path(__file__).resolve().parents[2]
PRIVATE_TARGETS = frozenset({("sve-engine", "cards"), ("sve-engine", "shared")})
FULL_THRESHOLD = 90
MAX_THRESHOLD = 100
EXPECTED_ARGS = 2


def _executable(name: str) -> str:
    value = shutil.which(name)
    if value is None:
        raise FileNotFoundError("Required CI tool is unavailable")
    return value


def mapping(value: object) -> dict[str, object]:
    """Keep arbitrary JSON types outside the scope logic."""
    if not isinstance(value, dict) or not all(isinstance(k, str) for k in value):
        msg = "Invalid test-scope input"
        raise ValueError(msg)
    return cast("dict[str, object]", value)


def text(value: object) -> str:
    """Only single-line values may enter Actions environment files."""
    if not isinstance(value, str) or not value or "\n" in value or "\r" in value:
        msg = "Invalid test-scope string"
        raise ValueError(msg)
    return value


def scope(event_name: str, repository: str, event: object) -> str:
    """Classify by repository provenance, not by credential availability."""
    document = mapping(event)
    if text(mapping(document.get("repository")).get("full_name")) != repository:
        msg = "Event repository does not match the workflow repository"
        raise ValueError(msg)
    if event_name == "push":
        if document.get("ref") != "refs/heads/main":
            msg = "Only main pushes have a trusted test scope"
            raise ValueError(msg)
        return "full"
    if event_name != "pull_request":
        msg = "Unknown test-scope event"
        raise ValueError(msg)
    request = mapping(document.get("pull_request"))
    base = mapping(mapping(request.get("base")).get("repo"))
    head = mapping(mapping(request.get("head")).get("repo"))
    if text(base.get("full_name")) != repository:
        msg = "Pull request base does not match the workflow repository"
        raise ValueError(msg)
    return "full" if text(head.get("full_name")) == repository else "fork"


def threshold(component: str, selected: str) -> int:
    """Fork coverage has its own measured gate."""
    if component not in {"python", "rust"} or selected not in {"full", "fork"}:
        msg = "Unknown coverage component or scope"
        raise ValueError(msg)
    if selected == "full":
        return FULL_THRESHOLD
    values = mapping(
        json.loads((ROOT / ".github/ci/fork-coverage.json").read_text(encoding="utf-8"))
    )
    value = values.get(component)
    if type(value) is not int or not 1 <= value <= MAX_THRESHOLD:
        msg = "Invalid fork coverage threshold"
        raise ValueError(msg)
    return value


def pin() -> dict[str, str]:
    """Require a shared immutable commit for Python and Rust."""
    with (ROOT / ".github/ci/testdata.lock").open("rb") as source:
        raw = tomllib.load(source)
    result = {
        key: text(raw.get(key))
        for key in ("repo", "commit", "snapshot_path", "snapshot_sha256")
    }
    if (
        not re.fullmatch(r"[A-Za-z0-9_-]+/[A-Za-z0-9_.-]+", result["repo"])
        or not re.fullmatch(r"[0-9a-f]{40}", result["commit"])
        or not re.fullmatch(r"[0-9a-f]{64}", result["snapshot_sha256"])
        or PurePosixPath(result["snapshot_path"]).is_absolute()
        or ".." in PurePosixPath(result["snapshot_path"]).parts
    ):
        msg = "Invalid private test-data pin"
        raise ValueError(msg)
    index = mapping(
        json.loads(
            (ROOT / "carddb/tests/fixtures/private-pages.json").read_text(
                encoding="utf-8"
            )
        )
    )
    if index.get("commit") != result["commit"]:
        msg = "Python and Rust private-data commits do not match"
        raise ValueError(msg)
    return result


def append_environment(values: dict[str, str]) -> None:
    """Publish only validated configuration, never private input bytes."""
    with Path(os.environ["GITHUB_ENV"]).open("a", encoding="utf-8") as output:
        output.writelines(f"{key}={text(value)}\n" for key, value in values.items())


def configure(component: str) -> None:
    """Bind this job to the event provenance before any private checkout."""
    selected = scope(
        os.environ["GITHUB_EVENT_NAME"],
        os.environ["GITHUB_REPOSITORY"],
        json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8")),
    )
    values = pin()
    mode = "required" if selected == "full" else "excluded"
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
        output.writelines(
            f"{key}={value}\n" for key, value in {**values, "mode": mode}.items()
        )
    append_environment(
        {
            "SVE_PRIVATE_TESTDATA_MODE": mode,
            "SVE_CI_TEST_MODE": selected,
            "SVE_CI_COVERAGE_THRESHOLD": str(threshold(component, selected)),
        }
    )
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        description = (
            "Full test scope; private test data required."
            if selected == "full"
            else "Fork / remaining-test scope. 私有真實頁測試未執行。 This is not full coverage acceptance."
        )
        with Path(summary).open("a", encoding="utf-8") as output:
            output.write(f"### {component.title()} test scope\n\n{description}\n\n")
    sys.stdout.write(f"Private data mode: {mode}; test scope: {selected}\n")


def verify(component: str) -> None:
    """Fail closed on trusted jobs with missing or changed inputs."""
    if os.environ.get("SVE_PRIVATE_TESTDATA_MODE") != "required":
        msg = "Private-data verification requires the trusted scope"
        raise ValueError(msg)
    values = pin()
    directory = ROOT / ".testdata"
    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed Git argv
        [_executable("git"), "-C", str(directory), "rev-parse", "HEAD"],
        capture_output=True,
        check=False,
        text=True,
    )
    if result.returncode or result.stdout.strip() != values["commit"]:
        msg = "Private test-data commit verification failed"
        raise ValueError(msg)
    if component == "python":
        # C1 owns all 19 per-file and field hashes; avoid a second oracle here.
        append_environment({"SVE_PRIVATE_TESTDATA_DIR": str(directory)})
    elif component == "rust":
        snapshot = directory / values["snapshot_path"]
        with snapshot.open("rb") as source:
            actual = hashlib.file_digest(source, "sha256").hexdigest()
        if actual != values["snapshot_sha256"]:
            msg = "Private engine snapshot verification failed"
            raise ValueError(msg)
        append_environment({"SVE_TEST_SNAPSHOT": str(snapshot)})
    else:
        msg = "Unknown private-data component"
        raise ValueError(msg)


def _test_name(value: object) -> str | None:
    target = mapping(value)
    if target.get("kind") != ["test"]:
        return None
    name = text(target.get("name"))
    if not re.fullmatch(r"[A-Za-z0-9_-]+", name):
        raise ValueError("Invalid Cargo test target name")
    return name


def public_rust_targets(document: object) -> list[str]:
    """Keep public workspace tests while excluding only the two private targets."""
    metadata = mapping(document)
    members = metadata.get("workspace_members")
    packages = metadata.get("packages")
    if not isinstance(members, list) or not isinstance(packages, list):
        msg = "Invalid Cargo workspace metadata"
        raise TypeError(msg)
    selected: set[str] = set()
    private: set[tuple[str, str]] = set()
    for item in packages:
        package = mapping(item)
        if package.get("id") not in members:
            continue
        name = text(package.get("name"))
        targets = package.get("targets")
        if not isinstance(targets, list):
            msg = "Invalid Cargo test targets"
            raise TypeError(msg)
        for item_target in targets:
            target_name = _test_name(item_target)
            if target_name is None:
                continue
            key = (name, target_name)
            if key in PRIVATE_TARGETS:
                private.add(key)
            else:
                selected.add(target_name)
    if private != PRIVATE_TARGETS or not selected:
        msg = "Private/public Cargo test target inventory changed"
        raise ValueError(msg)
    if selected & {name for _, name in PRIVATE_TARGETS}:
        raise ValueError("Ambiguous Cargo private test targets")
    return sorted(selected)


def rust_command(selected: str, minimum: int, report: Path) -> list[str]:
    """Keep production coverage scope intact when private tests are unavailable."""
    if selected not in {"full", "fork"}:
        msg = "Unknown Rust test scope"
        raise ValueError(msg)
    command = [
        _executable("cargo"),
        "llvm-cov",
        "--locked",
        "--workspace",
        "-j",
        "4",
        "--summary-only",
        "--fail-under-lines",
        str(minimum),
        "--json",
        "--output-path",
        str(report),
    ]
    if selected == "fork":
        metadata = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed local metadata query
            [
                _executable("cargo"),
                "metadata",
                "--locked",
                "--offline",
                "--no-deps",
                "--format-version",
                "1",
            ],
            capture_output=True,
            check=True,
            text=True,
        )
        command += ["--lib", "--bins"]
        for name in public_rust_targets(json.loads(metadata.stdout)):
            command += ["--test", name]
    return command


def main(argv: list[str]) -> int:
    """Keep parser/tool diagnostics out of public job logs."""
    try:
        if len(argv) == EXPECTED_ARGS and argv[0] in {"configure", "verify"}:
            if argv[0] == "configure":
                configure(argv[1])
            else:
                verify(argv[1])
            return 0
        if len(argv) == EXPECTED_ARGS and argv[0] == "rust-tests":
            selected = os.environ["SVE_CI_TEST_MODE"]
            minimum = threshold("rust", selected)
            environment = dict(os.environ)
            if selected == "fork":
                environment.pop("SVE_TEST_SNAPSHOT", None)
            result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- validated argv without shell expansion
                rust_command(selected, minimum, Path(argv[1])),
                env=environment,
                check=False,
            )
            return result.returncode
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
        sys.stderr.write("Test scope or private test-data verification failed.\n")
        return 1
    sys.stderr.write(
        "Usage: testdata.py configure|verify python|rust; rust-tests REPORT\n"
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
