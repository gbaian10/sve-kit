"""Versioned printing int_id allocation policy shared by the tool and its checks."""

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.domains.registry.storage import Entry

ALLOCATION_POLICY = "region-ranges-2026-09-28-v1"


@dataclass(frozen=True)
class IdRange:
    """Closed interval; ``end + 1`` is the exhausted cursor, never an allocatable ID."""

    start: int
    end: int

    def __contains__(self, value: object) -> bool:
        """Accept only real ints inside the closed interval."""
        return type(value) is int and self.start <= value <= self.end


# 1..20000 is reserved and never used by the regular allocator; 60000 and
# 100000..4294967295 are unassigned so new regions get new disjoint ranges.
RESERVED = IdRange(1, 20000)
REGION_RANGES = {"jp": IdRange(20001, 59999), "en": IdRange(60001, 99999)}


def region_range(region: str) -> IdRange:
    """Look up the range by registered printing region, never by the number itself."""
    if region not in REGION_RANGES:
        raise ValueError(f"No int_id range for region: {region}")
    return REGION_RANGES[region]


def cursors(allocated: dict[str, list[int]]) -> dict[str, int]:
    """Next free value per region: its highest allocation + 1, or the range start."""
    unknown = set(allocated) - set(REGION_RANGES)
    if unknown:
        raise ValueError(f"No int_id range for region: {sorted(unknown)}")
    return {
        region: max(allocated.get(region, []), default=bounds.start - 1) + 1
        for region, bounds in sorted(REGION_RANGES.items())
    }


def region_allocations(entries: list[Entry]) -> dict[str, list[int]]:
    """Group allocated int_ids by the registered region of their printing."""
    regions = {
        str(entry.data["id"]): entry.data["region"]
        for entry in entries
        if entry.kind == "printing"
    }
    grouped: dict[str, list[int]] = defaultdict(list)
    for entry in entries:
        if entry.kind == "card_int_id":
            value = entry.data["int_id"]
            region = regions.get(str(entry.data["printing_id"]))
            if type(value) is not int or not isinstance(region, str):
                raise ValueError(f"Invalid int_id allocation: {entry.record_key}")
            grouped[region].append(value)
    return grouped
