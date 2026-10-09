"""Same-card digital candidates retain fixed source priority and ambiguity refusal."""

import pytest

from sve_carddb.domains.translations.names.counterparts import (
    NameCandidate,
    first_counterpart,
)


def candidate(text: str, origin: str = "machine") -> NameCandidate:
    """Synthetic candidates represent already checked evidence, never adopted rows."""
    return NameCandidate(
        text,
        origin,
        "digital_official" if origin.startswith("official_") else "unofficial",
        "synthetic-decision",
        None,
    )


def test_first_counterpart_priority_and_ambiguity() -> None:
    first = candidate("First source", "official_sv1")
    second = candidate("Second source", "official_svwb")
    assert first_counterpart((second, first)) == first
    assert first_counterpart(()) is None
    assert first_counterpart((second,)) == second
    with pytest.raises(ValueError, match=r"^Ambiguous adopted digital names$"):
        first_counterpart((first, candidate("Another name", "official_sv1")))
