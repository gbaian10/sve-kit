"""Many-to-many raw positions for the unchanged legacy normalization recipe."""

import unicodedata
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from sve_carddb.contracts.template_parameters import Range
from sve_carddb.core.json import digest
from sve_carddb.template_sources.normalizer import DIGITS, QUOTED

if TYPE_CHECKING:
    from sve_carddb.template_sources.normalizer import Part


@dataclass(frozen=True)
class Unit:
    text: str
    origins: tuple[Range, ...]
    transformation: Literal["literal", "digits", "quoted"] = "literal"


def merged(origins: tuple[Range, ...]) -> tuple[Range, ...]:
    """One compatibility character can contribute to several normalized positions."""
    result: list[Range] = []
    for span in sorted(origins, key=lambda item: (item.start, item.end)):
        if result and span.start <= result[-1].end:
            result[-1] = Range(
                start=result[-1].start, end=max(result[-1].end, span.end)
            )
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
                Range(start=position, end=position + 1),
            )
        )
        units[common:] = [Unit(char, origins) for char in current[common:]]
        previous = current
    if "".join(unit.text for unit in units) != unicodedata.normalize("NFKC", text):
        raise ValueError("NFKC provenance differs from whole-string normalization")
    return tuple(units)


def trace(text: str, part: Part) -> tuple[Unit, ...]:
    """Keep quote delimiters literal and number/name replacements independently located."""
    positions = tuple(i for span in part.segments for i in range(span.start, span.end))
    raw = "".join(text[i] for i in positions)
    if part.role != "body":
        units = tuple(Unit(text[i], (Range(start=i, end=i + 1),)) for i in positions)
    else:
        units = nfkc(raw, positions)
        value = "".join(unit.text for unit in units)
        quoted: list[Unit] = []
        position = 0
        for match in QUOTED.finditer(value):
            quoted.extend(units[position : match.start() + 1])
            quoted.extend(
                (
                    Unit(
                        "X",
                        merged(
                            tuple(
                                s
                                for u in units[match.start() + 1 : match.end() - 1]
                                for s in u.origins
                            )
                        ),
                        "quoted",
                    ),
                    units[match.end() - 1],
                )
            )
            position = match.end()
        quoted.extend(units[position:])
        value = "".join(unit.text for unit in quoted)
        replaced = []
        position = 0
        for match in DIGITS.finditer(value):
            replaced.extend(quoted[position : match.start()])
            replaced.append(
                Unit(
                    "N",
                    merged(
                        tuple(
                            s
                            for u in quoted[match.start() : match.end()]
                            for s in u.origins
                        )
                    ),
                    "digits",
                )
            )
            position = match.end()
        replaced.extend(quoted[position:])
        units = tuple(replaced)
    if "".join(unit.text for unit in units) != part.normalized:
        raise ValueError(
            "Parameter provenance must reproduce the pinned normalized bytes"
        )
    covered = {i for unit in units for s in unit.origins for i in range(s.start, s.end)}
    if covered != set(positions):
        raise ValueError("Parameter provenance must retain every selected raw position")
    return units


def raw_value(text: str, unit: Unit) -> str:
    """Reconstruct exact source spellings in memory; candidate outputs use their hashes."""
    return "".join(text[s.start : s.end] for s in unit.origins)


def value_hash(text: str, unit: Unit) -> str:
    """Bind the raw spelling rather than inventing a reversible normalized value."""
    return digest(raw_value(text, unit).encode())
