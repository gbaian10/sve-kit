"""Printed same-name limits shared by author admission and snapshot projection."""

import re

from pydantic import JsonValue

CONDITIONAL_PATTERN = re.compile(
    r"カード名に『(?P<exclude>[^』]+)』を含まないカードは、"
    r"メインデッキやエボルヴデッキにそれぞれ(?P<n>\d+)枚までしか入れられない。"
)


def conditional_copy_rule(line: str) -> dict[str, JsonValue] | None:
    """Recognize the complete conditional construction sentence (Q2610–Q2612)."""
    match = CONDITIONAL_PATTERN.fullmatch(line.strip())
    if match is None:
        return None
    return {
        "when": "self_in_main",
        "decks": ["main", "evolve"],
        "exclude_name_contains": match["exclude"],
        "same_name_max": int(match["n"]),
    }


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
