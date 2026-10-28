"""Generate ephemeral public card inputs before every complete Rust test scope."""

import os
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- fixed tool argv; no shell
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT = [
    "llvm-cov",
    "--locked",
    "--workspace",
    "--summary-only",
    "--fail-under-lines",
    "90",
]


def executable(name: str) -> str:
    """Resolve an installed tool without shell interpolation."""
    path = shutil.which(name)
    if path is None:
        raise FileNotFoundError("Required test tool is unavailable")
    return path


def main(arguments: list[str]) -> int:
    """Never reuse a caller's private snapshot or a stale generated JSONL."""
    with tempfile.TemporaryDirectory(prefix="sve-engine-input-") as temporary:
        output = Path(temporary) / "cards.jsonl"
        selection = Path(temporary) / "selection.json"
        subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed local test adapter
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
            ],
            check=True,
        )
        environment = {
            **os.environ,
            "SVE_TEST_SNAPSHOT": str(output),
            "SVE_TEST_CURRENT_SELECTION": str(selection),
        }
        return subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- Cargo receives explicit argument boundaries
            [executable("mise"), "exec", "--", "cargo", *(arguments or DEFAULT)],
            cwd=ROOT,
            env=environment,
            check=False,
        ).returncode


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
