"""Finite examples of aggregate contracts, without claiming runtime execution."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Collection, Mapping

    from sve_carddb.contracts.four_layer import ObjectGeneration


def disjoint_pair_count(
    left: Collection[ObjectGeneration],
    right: Collection[ObjectGeneration],
) -> int:
    """A finite contract probe; this does not evaluate a DSL selector or pair event."""
    a, b = set(left), set(right)
    if len(a) != len(left) or len(b) != len(right) or a & b:
        raise ValueError(
            "Pair operands must be disjoint with each card used at most once"
        )
    return min(len(a), len(b))


def verify_counter_witness(
    candidates: Mapping[ObjectGeneration, int],
    first: ObjectGeneration | None,
    second: ObjectGeneration | None,
    thresholds: tuple[int, int] = (4, 10),
) -> tuple[bool, bool]:
    """K5 records a common witness for the highest satisfied sealed threshold."""
    if thresholds != (4, 10) or any(
        type(n) is not int or n < 0 for n in candidates.values()
    ):
        raise ValueError("Invalid sealed counter snapshot")
    outcomes = tuple(any(n >= t for n in candidates.values()) for t in thresholds)
    if first != second:
        raise ValueError("Counter reads require the same holder generation")
    if not outcomes[0]:
        if first is not None:
            raise ValueError("No holder witnesses an unmet threshold")
        return False, False
    if (
        first is None
        or first not in candidates
        or any(
            (candidates[first] >= t) != result
            for t, result in zip(thresholds, outcomes, strict=True)
        )
    ):
        raise ValueError("Holder does not witness the highest satisfied threshold")
    return outcomes[0], outcomes[1]
