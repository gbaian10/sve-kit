"""Stable numeric ranges; additions never rebalance previously reviewed cards."""

import re
from typing import TYPE_CHECKING

from sve_carddb.domains.parsed_cards.models import RANGE_WIDTH

if TYPE_CHECKING:
    from sve_carddb.core.regions import Region


def number_range(number: str, region: Region) -> str:
    """Use only the numeric suffix, never the package's number or a region match."""
    local = number.removesuffix("EN") if region == "en" else number
    _, separator, suffix = local.rpartition("-")
    if not separator:
        raise ValueError("Parsed card number has no range suffix")
    if suffix in {"EP", "SEP"}:
        return "unnumbered"
    match = re.fullmatch(r"([A-ZⓈ]*)([0-9]+)[a-z]*", suffix)
    if match is None:
        raise ValueError("Parsed card number requires an explicit range exception")
    start = int(match[2]) // RANGE_WIDTH * RANGE_WIDTH
    prefix = match[1].replace("Ⓢ", "_circled_s") + "/" if match[1] else ""
    return f"{prefix}{start:03d}-{start + RANGE_WIDTH - 1:03d}"


def file_path(region: Region, family: str, number: str) -> str:
    """Choose a reviewed family and fixed range without measuring file size."""
    return f"{region}/{family}/{number_range(number, region)}.json"
