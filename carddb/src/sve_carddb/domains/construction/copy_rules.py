"""Printed same-name limits shared by author admission and snapshot projection."""

import re

PATTERNS = {
    "jp": (
        (
            re.compile(
                r"『(?P<name>[^』]+)』は自分のデッキに(?P<n>\d+)枚まで入れることができる。"
            ),
            "main",
        ),
        (
            re.compile(r"これはエボルヴデッキに(?P<n>\d+)枚まで入れることができる。"),
            "evolve",
        ),
    ),
    "en": (
        (
            re.compile(r"You can put up to (?P<n>\d+) of this card into your deck\."),
            "main",
        ),
        (
            re.compile(
                r"You can put up to (?P<n>\d+) of this card into your evolve deck\."
            ),
            "evolve",
        ),
    ),
}


def copy_rule(line: str, region: str, own_name: str) -> tuple[str, int] | None:
    """Recognize only complete supported sentences naming this card."""
    for pattern, deck in PATTERNS[region]:
        match = pattern.fullmatch(line.strip())
        if match is None:
            continue
        named = match.groupdict().get("name")
        if named is not None and named != own_name:
            raise ValueError("A copy rule names another card")
        return deck, int(match["n"])
    return None


def has_copy_rule(text: str, region: str) -> bool:
    """Avoid requiring a rules name for fields without a complete copy sentence."""
    return any(
        pattern.fullmatch(line.strip())
        for line in text.splitlines()
        for pattern, _ in PATTERNS[region]
    )
