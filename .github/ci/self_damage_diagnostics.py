"""Temporary failure facts for the self-damage tests, excluding card wording."""

import re
import sys
from pathlib import Path


def safe_details(source: str) -> list[str]:
    """Only fixed diagnostic forms and bounded source locations may be public."""
    patterns = (
        (
            "panic location",
            r"panicked at ((?:sim/engine|sim/scenario-runner)/(?:[A-Za-z0-9_-]+/)*[A-Za-z0-9_-]+\.rs:[0-9]+:[0-9]+):",
        ),
        (
            "runner shape",
            r'reason: "(typed runner: (?:choose-pending|resolve-choice|activate|evolve|play) (?:at|for) [A-Za-z0-9_-]+)"',
        ),
        (
            "rejected answer",
            r'reason: "((?:choose-pending|resolve-choice|activate|evolve|play) answer rejected)"',
        ),
        (
            "missing authored program",
            r'reason: "(no authored program: [A-Z0-9-]{2,24})"',
        ),
        (
            "unloaded whole card",
            r'reason: "(internal D1 does not support unloaded whole card [A-Z0-9-]{2,24}/[0-9]+, (?:holder [A-Za-z0-9_-]{1,40} generation [0-9]+, )?query (?:restriction [A-Za-z ]+|complete card names): current ability definitions unavailable)"',
        ),
        (
            "missing pending line/source",
            r'pending \{\\"line\\":([0-9]+),\\"source\\":\\"([A-Za-z][A-Za-z0-9_-]{0,40})\\"\} is not offered',
        ),
        (
            "checkpoint/view/field",
            r'at: "(after-decision-[0-9]+)",\s*view: "(omniscient|P1|P2)",\s*what: "(outcome|awaiting|events|assert [A-Za-z0-9_.]+)"',
        ),
        (
            "numeric assertion",
            r"left:\s*(?:Number\()?(-?[0-9]+)\)?\s+right:\s*(?:Number\()?(-?[0-9]+)\)?",
        ),
    )
    details: list[str] = []
    for block in re.split(r"(?m)^---- ", source):
        header = block.partition("\n")[0]
        if not re.fullmatch(
            r"(?:runtime::self_damage::[A-Za-z0-9_]+|every_shared_scenario_passes_on_the_typed_entry_or_is_listed) stdout ----",
            header,
        ):
            continue
        name = header.removesuffix(" stdout ----")
        for label, pattern in patterns:
            details.extend(
                f"{name}: {label}: {' / '.join(match.groups())}"
                for match in re.finditer(pattern, block)
            )
    return list(dict.fromkeys(details))


def main() -> None:
    """Read the private log without echoing unrecognised diagnostic contents."""
    source = Path(sys.argv[1]).read_text(encoding="utf-8")
    for detail in safe_details(source):
        sys.stdout.write(f"safe self-damage diagnostic: {detail}\n")


if __name__ == "__main__":
    main()
