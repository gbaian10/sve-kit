"""Finite entry projections preserve complete heads and source operand order."""

from dataclasses import replace

import pytest

from sve_carddb.domains.effects.author_sources import (
    SourceContext,
    SourceError,
    entry_heads,
    entry_payload,
)
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_semantics import CardContext

from ..translations.test_four_layer_classification import classifier, source


def entry(raw: str) -> SourceContext:
    descriptor = source(raw)
    engine = classifier()
    derived = derive_source(
        raw,
        descriptor,
        engine,
        context=CardContext(
            descriptor, "card:synthetic", "face:synthetic", "normal", "follower"
        ),
    )
    frame, binding = derived.bind(0)
    return SourceContext(
        frame, binding, derived.field.parts[0].canonical_source, engine
    )


@pytest.mark.parametrize(
    ("raw", "heads", "costs"),
    [
        ("{進化}{コスト０}:これは進化する。", ("evolve",), (0,)),
        ("{食事}{コスト２}:これは出走する。", ("feed",), (2,)),
        ("{憑依}{コスト３}:これはドライブを持つ。", ("ride",), (3,)),
        (
            "{進化}{コスト１}:これは進化する。 {食事}{コスト２}:これは出走する。",
            ("evolve", "feed"),
            (1, 2),
        ),
        (
            "{進化}{コスト２}:これは進化する。 {憑依}{コスト１}:これはドライブを持つ。",
            ("evolve", "ride"),
            (2, 1),
        ),
        (
            "{進化}{コスト２}:これは進化する。 {食事}{コスト２}:これは出走する。",
            ("evolve", "feed"),
            (2, 2),
        ),
    ],
    ids=[
        "evolve-zero",
        "feed",
        "ride",
        "synthetic-dual-feed",
        "synthetic-dual-ride",
        "synthetic-equal-values",
    ],
)
def test_exact_entry_payloads(
    raw: str, heads: tuple[str, ...], costs: tuple[int, ...]
) -> None:
    context = entry(raw)
    assert entry_heads(context.frame) == heads
    for index, (head, cost) in enumerate(zip(heads, costs, strict=True)):
        assert entry_payload(context, f"ability_{index}") == {
            head: {"cost": {"pp": cost}},
            "ports": {"imports": {}, "exports": {}},
        }


def test_cost_uses_occurrence_identity_not_equal_value() -> None:
    context = entry(
        "{進化}{コスト２}:これは進化する。 {食事}{コスト２}:これは出走する。"
    )
    costs = {
        slot.name
        for slot in context.frame.leaf_schema.slots
        if slot.role == "cost_value"
    }
    occurrences = tuple(o for o in context.binding.occurrences if o.slot in costs)
    assert len(occurrences) == 2
    altered = replace(
        context,
        binding=context.binding.model_copy(
            update={
                "occurrences": tuple(
                    o for o in context.binding.occurrences if o != occurrences[1]
                )
            }
        ),
    )
    with pytest.raises(SourceError, match="source_cost_operand_mismatch"):
        entry_payload(altered, "ability_1")


@pytest.mark.parametrize(
    "change", ["pending", "discriminator", "head", "scope", "role", "kind"]
)
def test_entry_projection_rejects_unmapped_claims(change: str) -> None:
    context = entry("{進化}{コスト１}:これは進化する。")
    frame = context.frame
    if change == "pending":
        frame = frame.model_copy(
            update={
                "semantic_variant": frame.semantic_variant.model_copy(
                    update={"state": "pending", "key": None}
                )
            }
        )
    elif change == "head":
        frame = frame.model_copy(
            update={
                "semantic_variant": frame.semantic_variant.model_copy(
                    update={"key": "future_entry.v1"}
                )
            }
        )
    elif change == "role":
        frame = frame.model_copy(update={"role": "label"})
    else:
        updates = (
            {"discriminator": "future_entry.v1"}
            if change == "discriminator"
            else {"scopes": ()}
            if change == "scope"
            else {"projection_kind": "card_field"}
        )
        frame = frame.model_copy(
            update={"projection": frame.projection.model_copy(update=updates)}
        )
    with pytest.raises(SourceError):
        entry_heads(frame)
