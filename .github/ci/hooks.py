# /// script
# requires-python = ">=3.12"
# dependencies = ["pyyaml>=6.0.3"]
# ///
"""Map pre-commit hooks to CI jobs using .github/ci/hooks.toml.

Usage:
  uv run .github/ci/hooks.py check
  uv run .github/ci/hooks.py skip JOB   # SKIP value for `pre-commit run` in that job
"""

import re
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import cast

import yaml

ROOT = Path(__file__).resolve().parents[2]
JOBS = frozenset({"repo", "python", "rust", "web", "commit", "none"})
# The repo job runs `pre-commit run --hook-stage pre-commit`; its hooks must run at that stage.
REPO_STAGE = "pre-commit"
UV_HOOK_REPO = "https://github.com/astral-sh/uv-pre-commit"


@dataclass(frozen=True)
class Hook:
    """One hook entry of .pre-commit-config.yaml."""

    id: str
    stages: tuple[str, ...]


@dataclass(frozen=True)
class Owner:
    """One entry of hooks.toml."""

    job: str
    reason: str


def load_config() -> dict[str, object]:
    """Parse .pre-commit-config.yaml."""
    text = (ROOT / ".pre-commit-config.yaml").read_text()
    return cast("dict[str, object]", yaml.safe_load(text))


def configured_hooks(config: dict[str, object]) -> list[Hook]:
    """Return the hooks in file order, with stages falling back to default_stages."""
    default = tuple(cast("list[str]", config.get("default_stages", [])))
    repos = cast("list[dict[str, object]]", config["repos"])
    return [
        Hook(
            id=str(hook["id"]),
            stages=tuple(cast("list[str]", hook.get("stages", default))),
        )
        for repo in repos
        for hook in cast("list[dict[str, object]]", repo["hooks"])
    ]


def owners() -> dict[str, Owner]:
    """Return the CI job (and reason) that owns each hook."""
    table = tomllib.loads((ROOT / ".github/ci/hooks.toml").read_text())
    hooks = cast("dict[str, dict[str, str]]", table["hooks"])
    return {
        hook: Owner(job=entry["job"], reason=entry.get("reason", "").strip())
        for hook, entry in hooks.items()
    }


def uv_problems(config: dict[str, object]) -> list[str]:
    """Check that CI's uv (carddb required-version) matches the uv-lock hook rev."""
    repos = cast("list[dict[str, object]]", config["repos"])
    revs = [str(repo["rev"]) for repo in repos if repo.get("repo") == UV_HOOK_REPO]
    pyproject = tomllib.loads((ROOT / "carddb/pyproject.toml").read_text())
    tool = cast("dict[str, dict[str, str]]", pyproject.get("tool", {}))
    required = tool.get("uv", {}).get("required-version", "")
    match = re.fullmatch(r"==(\S+)", required)
    if not match:
        return [
            f"carddb [tool.uv] required-version must pin one version, got {required!r}"
        ]
    if revs != [match.group(1)]:
        return [
            f"uv-pre-commit rev {revs} differs from carddb required-version {required!r}"
        ]
    return []


def problems(hooks: list[Hook], owned: dict[str, Owner]) -> list[str]:
    """Return every mismatch between the hook config and the ownership table."""
    ids = [hook.id for hook in hooks]
    found = [
        f"hook {hook_id!r} has no CI job in .github/ci/hooks.toml"
        for hook_id in ids
        if hook_id not in owned
    ]
    found += [
        f"hooks.toml lists {hook_id!r}, which is not in .pre-commit-config.yaml"
        for hook_id in owned
        if hook_id not in ids
    ]
    found += [
        f"hook {hook_id!r} has unknown job {owner.job!r}"
        for hook_id, owner in owned.items()
        if owner.job not in JOBS
    ]
    found += [
        f"hook {hook_id!r} is not run by CI but gives no reason"
        for hook_id, owner in owned.items()
        if owner.job == "none" and not owner.reason
    ]
    found += [
        f"hook {hook.id!r} belongs to the repo job but does not run at stage {REPO_STAGE!r}"
        for hook in hooks
        if hook.id in owned
        and owned[hook.id].job == "repo"
        and REPO_STAGE not in hook.stages
    ]
    duplicates = {hook_id for hook_id in ids if ids.count(hook_id) > 1}
    found += [
        f"hook id {hook_id!r} appears more than once" for hook_id in sorted(duplicates)
    ]
    return found


def main(argv: list[str]) -> int:
    """Run the subcommand in argv and return the exit status."""
    config = load_config()
    hooks, owned = configured_hooks(config), owners()
    if errors := problems(hooks, owned) + uv_problems(config):
        sys.stderr.write("\n".join(errors) + "\n")
        return 1
    match argv:
        case ["check"]:
            return 0
        case ["skip", job] if job in JOBS - {"none"}:
            skipped = (hook.id for hook in hooks if owned[hook.id].job != job)
            sys.stdout.write(",".join(skipped) + "\n")
            return 0
        case _:
            sys.stderr.write(__doc__ or "")
            return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
