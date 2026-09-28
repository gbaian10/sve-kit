# /// script
# requires-python = ">=3.12"
# dependencies = ["pyyaml>=6.0.3"]
# ///
"""Map pre-commit hooks to CI jobs using .github/ci/hooks.toml.

Usage:
  uv run .github/ci/hooks.py check
  uv run .github/ci/hooks.py skip JOB   # SKIP value for `pre-commit run` in that job
"""

import sys
import tomllib
from pathlib import Path
from typing import cast

import yaml

ROOT = Path(__file__).resolve().parents[2]
JOBS = frozenset({"repo", "python", "rust", "web", "commit", "none"})


def configured_hooks() -> list[str]:
    """Return hook ids in .pre-commit-config.yaml, in file order."""
    config = cast(
        "dict[str, object]",
        yaml.safe_load((ROOT / ".pre-commit-config.yaml").read_text()),
    )
    repos = cast("list[dict[str, object]]", config["repos"])
    return [
        str(hook["id"])
        for repo in repos
        for hook in cast("list[dict[str, object]]", repo["hooks"])
    ]


def owners() -> dict[str, str]:
    """Return the CI job that owns each hook."""
    table = tomllib.loads((ROOT / ".github/ci/hooks.toml").read_text())
    hooks = cast("dict[str, dict[str, str]]", table["hooks"])
    return {hook: entry["job"] for hook, entry in hooks.items()}


def problems(hooks: list[str], owned: dict[str, str]) -> list[str]:
    """Return every mismatch between the hook config and the ownership table."""
    found = [
        f"hook {hook!r} has no CI job in .github/ci/hooks.toml"
        for hook in hooks
        if hook not in owned
    ]
    found += [
        f"hooks.toml lists {hook!r}, which is not in .pre-commit-config.yaml"
        for hook in owned
        if hook not in hooks
    ]
    found += [
        f"hook {hook!r} has unknown job {job!r}"
        for hook, job in owned.items()
        if job not in JOBS
    ]
    duplicates = {hook for hook in hooks if hooks.count(hook) > 1}
    found += [f"hook id {hook!r} appears more than once" for hook in sorted(duplicates)]
    return found


def main(argv: list[str]) -> int:
    """Run the subcommand in argv and return the exit status."""
    hooks, owned = configured_hooks(), owners()
    if errors := problems(hooks, owned):
        sys.stderr.write("\n".join(errors) + "\n")
        return 1
    match argv:
        case ["check"]:
            return 0
        case ["skip", job] if job in JOBS - {"none"}:
            sys.stdout.write(
                ",".join(hook for hook in hooks if owned[hook] != job) + "\n"
            )
            return 0
        case _:
            sys.stderr.write(__doc__ or "")
            return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
