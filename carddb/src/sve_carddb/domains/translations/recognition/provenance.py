"""Many-to-many raw positions for the authoritative source normalization."""

import unicodedata
from dataclasses import dataclass
from typing import Literal

from sve_carddb.contracts.four_layer import Span


@dataclass(frozen=True)
class Unit:
    text: str
    origins: tuple[Span, ...]
    transformation: Literal["literal", "digits", "quoted"] = "literal"


def merged(origins: tuple[Span, ...]) -> tuple[Span, ...]:
    """One compatibility character can contribute to several normalized positions."""
    result: list[Span] = []
    for span in sorted(origins, key=lambda item: (item.start, item.end)):
        if result and span.start <= result[-1].end:
            result[-1] = Span(start=result[-1].start, end=max(result[-1].end, span.end))
        else:
            result.append(span)
    return tuple(result)


def nfkc(text: str, positions: tuple[int, ...]) -> tuple[Unit, ...]:
    """Track changes to whole prefixes, including composition and canonical reordering."""
    if len(text) != len(positions):
        raise ValueError("NFKC provenance requires one position per raw code point")
    units: list[Unit] = []
    previous = ""
    for index, position in enumerate(positions):
        current = unicodedata.normalize("NFKC", text[: index + 1])
        common = 0
        for before, after in zip(previous, current, strict=False):
            if before != after:
                break
            common += 1
        origins = merged(
            (
                *(span for unit in units[common:] for span in unit.origins),
                Span(start=position, end=position + 1),
            )
        )
        units[common:] = [Unit(char, origins) for char in current[common:]]
        previous = current
    if "".join(unit.text for unit in units) != unicodedata.normalize("NFKC", text):
        raise ValueError("NFKC provenance differs from whole-string normalization")
    return tuple(units)


def raw_value(text: str, unit: Unit) -> str:
    """Reconstruct exact source spellings in memory; candidate outputs use their hashes."""
    return "".join(text[s.start : s.end] for s in unit.origins)
