"""Report callable/import dead code without Pydantic field false positives."""

# ruff: file-ignore[suspicious-subprocess-import, subprocess-without-shell-equals-true, start-process-with-partial-path] -- fixed uv command runs the locked development tool

import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def main(argv: list[str]) -> int:
    """Scan one scope, preserving Vulture errors even when no finding matches."""
    if argv not in (["dev"], ["prod"]):
        sys.stderr.write("Usage: vulture.py dev|prod\n")
        return 2
    scope = "tests" if argv == ["dev"] else "src"
    # Vulture CLI paths replace configured paths, so append the selected scope here.
    with (ROOT / "carddb/pyproject.toml").open("rb") as config_file:
        paths = tomllib.load(config_file)["tool"]["vulture"]["paths"]
    result = subprocess.run(
        ["uv", "run", "vulture", str(ROOT / "carddb" / scope), *paths],
        cwd=ROOT / "carddb",
        check=False,
        capture_output=True,
        text=True,
    )
    sys.stderr.write(result.stderr)
    # Vulture can overwrite a syntax-error status with its generic findings status.
    if result.stderr or result.returncode not in {0, 3}:
        sys.stderr.write(result.stdout)
        return result.returncode or 1
    findings = [
        line
        for line in result.stdout.splitlines()
        if re.search(r": unused (function|method|class|import) ", line)
    ]
    for line in findings:
        sys.stdout.write(line + "\n")
    return int(bool(findings))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
