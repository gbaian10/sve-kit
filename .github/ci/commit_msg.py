"""Check commit messages against the commitizen rules with the gitmoji made mandatory.

cz-conventional-gitmoji leaves the gitmoji optional (the local gitmojify hook adds it), but a
squash merge on GitHub runs no local hook, so CI must reject a message without one. A leading
GitHub shortcode such as `:arrow_up:` counts as its gitmoji: Dependabot writes commits that way.

Usage (run through the carddb dev environment, which has commitizen):
  uv run --project carddb --no-default-groups --group dev python .github/ci/commit_msg.py --message TEXT
"""

import argparse
import re
import sys

from commitizen.config import read_cfg
from commitizen.factory import committer_factory
from shared.utils import get_gitmojis

# The pattern wraps each gitmoji in an optional group such as `(✨ {1,2})?feat`; drop the `?`.
_OPTIONAL_EMOJI = re.compile(r"(\(\S+ \{1,2\}\))\?")


def strict_pattern() -> re.Pattern[str]:
    """Return the commitizen schema pattern with the gitmoji required."""
    committer = committer_factory(read_cfg())
    return re.compile(_OPTIONAL_EMOJI.sub(r"\1", committer.schema_pattern()))


def with_icon(text: str) -> str:
    """Replace a leading gitmoji shortcode (`:arrow_up: …`) with its emoji."""
    for moji in get_gitmojis():
        if text.startswith(f"{moji.code} "):
            return moji.icon + text.removeprefix(moji.code)
    return text


def allowed_prefixes() -> tuple[str, ...]:
    """Return the prefixes commitizen lets through unchecked (Merge, Revert, fixup! and so on)."""
    prefixes = read_cfg().settings.get("allowed_prefixes", [])
    return tuple(str(prefix) for prefix in prefixes)


def main(argv: list[str]) -> int:
    """Check the given message and return the exit status."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--message", required=True)
    args = parser.parse_args(argv)

    pattern, prefixes = strict_pattern(), allowed_prefixes()
    message = args.message
    if message.startswith(prefixes) or pattern.fullmatch(with_icon(message)):
        return 0
    first_line = message.splitlines()[0] if message else ""
    sys.stderr.write(
        f"message: needs '<gitmoji> <type>(<scope>): <description>': {first_line}\n"
    )
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
