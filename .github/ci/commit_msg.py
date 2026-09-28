"""Check commit messages against the commitizen rules with the gitmoji made mandatory.

cz-conventional-gitmoji leaves the gitmoji optional (the local gitmojify hook adds it), but a
squash merge on GitHub runs no local hook, so CI must reject a message without one.

Usage (run through the carddb dev environment, which has commitizen):
  uv run --project carddb --no-default-groups --group dev python .github/ci/commit_msg.py --message TEXT
  uv run --project carddb --no-default-groups --group dev python .github/ci/commit_msg.py --rev-range A..B
"""

import argparse
import re
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- reads git log only
import sys

from commitizen.config import read_cfg
from commitizen.factory import committer_factory

# The pattern wraps each gitmoji in an optional group such as `(✨ {1,2})?feat`; drop the `?`.
_OPTIONAL_EMOJI = re.compile(r"(\(\S+ \{1,2\}\))\?")


def strict_pattern() -> re.Pattern[str]:
    """Return the commitizen schema pattern with the gitmoji required."""
    committer = committer_factory(read_cfg())
    return re.compile(_OPTIONAL_EMOJI.sub(r"\1", committer.schema_pattern()))


def allowed_prefixes() -> tuple[str, ...]:
    """Return the prefixes commitizen lets through unchecked (Merge, Revert, fixup! and so on)."""
    prefixes = read_cfg().settings.get("allowed_prefixes", [])
    return tuple(str(prefix) for prefix in prefixes)


def messages_in(rev_range: str) -> list[tuple[str, str]]:
    """Return (sha, full message) for every commit in the range, oldest first."""
    git = shutil.which("git") or "git"
    log = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed argv, no shell
        [git, "log", "--reverse", "--format=%H%x00%B%x1e", rev_range],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    result: list[tuple[str, str]] = []
    for entry in log.split("\x1e"):
        if entry.strip():
            sha, _, message = entry.strip("\n").partition("\x00")
            result.append((sha, message))
    return result


def main(argv: list[str]) -> int:
    """Check the given message or commit range and return the exit status."""
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--message")
    group.add_argument("--rev-range")
    args = parser.parse_args(argv)

    pattern, prefixes = strict_pattern(), allowed_prefixes()
    items = (
        [("message", args.message)]
        if args.message is not None
        else messages_in(args.rev_range)
    )
    failures = [
        (ref, text.splitlines()[0] if text else "")
        for ref, text in items
        if not text.startswith(prefixes) and not pattern.fullmatch(text)
    ]
    for ref, first_line in failures:
        sys.stderr.write(
            f"{ref}: needs '<gitmoji> <type>(<scope>): <description>': {first_line}\n"
        )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
