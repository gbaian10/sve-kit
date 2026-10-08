"""Exact printing coverage for immutable review decisions."""

from pydantic import JsonValue

from sve_carddb.domains.registry.inputs import canonical


def require_evidence(actual: JsonValue, expected: list[JsonValue]) -> None:
    """Reject uncovered or changed observations rather than extend old approval."""
    if not isinstance(actual, list):
        raise TypeError("Invalid evidence; requires re-review")
    observed = [canonical(item) for item in actual]
    required = [canonical(item) for item in expected]
    if len(observed) != len(set(observed)) or sorted(observed) != sorted(required):
        raise ValueError(
            "Printing evidence coverage mismatch; requires re-review: "
            "versioned review/relation decisions are not supported"
        )
