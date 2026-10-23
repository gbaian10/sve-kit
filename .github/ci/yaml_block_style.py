"""Reject YAML flow collections ({...} or [...]) that span several lines.

bun fmt keeps the style it is given and wraps an over-long flow collection across lines; such a
collection belongs in block style. The YAML scanner tells real collections from braces inside
strings and comments.
"""

import sys
from pathlib import Path

import yaml

OPEN = (yaml.FlowMappingStartToken, yaml.FlowSequenceStartToken)
CLOSE = (yaml.FlowMappingEndToken, yaml.FlowSequenceEndToken)


def wrapped_collections(text: str) -> list[int]:
    """Return the 1-based lines where a flow collection opens and closes on a later line."""
    starts: list[int] = []
    wrapped: list[int] = []
    for token in yaml.scan(text, Loader=yaml.SafeLoader):
        if isinstance(token, OPEN):
            starts.append(token.start_mark.line)
        elif isinstance(token, CLOSE):
            start = starts.pop()
            if token.end_mark.line != start:
                wrapped.append(start + 1)
    return wrapped


def main(paths: list[str]) -> int:
    """Report every wrapped collection in the given files; exit 1 if there is any."""
    failed = 0
    for path in paths:
        for line in wrapped_collections(Path(path).read_text(encoding="utf-8")):
            sys.stdout.write(
                f"{path}:{line}: write this collection in block style, one per line\n"
            )
            failed = 1
    return failed


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
