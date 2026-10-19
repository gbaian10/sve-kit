"""Select explicit test scopes without treating missing credentials as a fork."""

import json
import os
import re
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- fixed local Git commands
import sys
import tomllib
from pathlib import Path
from typing import cast

ROOT = Path(__file__).resolve().parents[2]
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
    if component != "python" or selected not in {"full", "fork"}:
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
    """Pin only the remaining private Python cases, independently of engine inputs."""
    with (ROOT / ".github/ci/testdata.lock").open("rb") as source:
        raw = tomllib.load(source)
    result = {key: text(raw.get(key)) for key in ("repo", "commit")}
    if not re.fullmatch(
        r"[A-Za-z0-9_-]+/[A-Za-z0-9_.-]+", result["repo"]
    ) or not re.fullmatch(r"[0-9a-f]{40}", result["commit"]):
        raise ValueError("Invalid private test-data pin")
    index = mapping(
        json.loads((ROOT / "carddb/tests/fixtures/private-pages.json").read_bytes())
    )
    if index.get("commit") != result["commit"]:
        raise ValueError("Private page index and checkout pins do not match")
    return result


def append_environment(values: dict[str, str]) -> None:
    """Publish only validated configuration, never private input bytes."""
    with Path(os.environ["GITHUB_ENV"]).open("a", encoding="utf-8") as output:
        output.writelines(f"{key}={text(value)}\n" for key, value in values.items())


def configure(component: str) -> None:
    """Bind this job to the event provenance before any private checkout."""
    if component != "python":
        raise ValueError("Only Python requires private test data")
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
            "Full Python scope; three other-game private fixtures required."
            if selected == "full"
            else "Fork / remaining-test scope; public SVE fixtures run. 私有真實頁測試未執行（其他遊戲 3 案例）。 This is not full coverage acceptance."
        )
        with Path(summary).open("a", encoding="utf-8") as output:
            output.write(f"### {component.title()} test scope\n\n{description}\n\n")
    sys.stdout.write(f"Private data mode: {mode}; test scope: {selected}\n")


def verify(component: str) -> None:
    """Fail closed on trusted jobs with missing or changed inputs."""
    if (
        component != "python"
        or os.environ.get("SVE_PRIVATE_TESTDATA_MODE") != "required"
    ):
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
    # Page tests own per-file and field hashes; CI only pins the checkout commit.
    append_environment({"SVE_PRIVATE_TESTDATA_DIR": str(directory)})


def main(argv: list[str]) -> int:
    """Keep private parser and checkout diagnostics out of public job logs."""
    try:
        if len(argv) == EXPECTED_ARGS and argv[1] == "python":
            if argv[0] == "configure":
                configure("python")
                return 0
            if argv[0] == "verify":
                verify("python")
                return 0
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
        sys.stderr.write("Test scope or private test-data verification failed.\n")
        return 1
    sys.stderr.write("Usage: testdata.py configure|verify python\n")
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
