"""Exact vocabulary lookups retain ambiguity even for identical bindings."""

import pytest

from sve_carddb.domains.text_observations import Binding, Vocabulary


@pytest.mark.parametrize("code", ["follower", "spell"])
def test_lookup_rejects_duplicate_raw_keys(code: str) -> None:
    binding = Binding(region="jp", kind="type", raw="Synthetic", code="follower")
    vocabulary = Vocabulary(
        bindings=(binding, binding.model_copy(update={"code": code}))
    )
    with pytest.raises(ValueError, match="Missing or ambiguous"):
        vocabulary.lookup("jp", "type", "Synthetic")
    with pytest.raises(ValueError, match="Duplicate vocabulary binding"):
        vocabulary.verify()


def test_lookup_keeps_region_kind_and_exact_raw_separate() -> None:
    bindings = (
        Binding(region="jp", kind="type", raw="Synthetic", code="follower"),
        Binding(region="en", kind="type", raw="Synthetic", code="spell"),
        Binding(region="jp", kind="class", raw="Synthetic", code="neutral"),
        Binding(region="jp", kind="type", raw="Synthetic ", code="amulet"),
    )
    vocabulary = Vocabulary(bindings=bindings)
    original = vocabulary.model_dump(mode="json")
    for binding in bindings:
        assert vocabulary.lookup(binding.region, binding.kind, binding.raw) == binding
    assert vocabulary.model_dump(mode="json") == original
