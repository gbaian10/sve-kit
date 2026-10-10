"""Repeated target references retain independently proven source roles and values."""

import pytest

from sve_carddb.contracts.four_layer import Frame, LeafSchema, hash_payload
from sve_carddb.domains.translations.four_layer_matching import Frames
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source

from .test_four_layer_classification import classifier, source


def folded(raw: str, *, split_roles: bool = False) -> tuple[Frame, str]:
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = classifier().recognize(raw, field.source, part, field=field)
    frame, _ = found.bind(field.source, part)
    slots = found.schema.slots
    assert len(slots) == 2
    updated = frame.model_copy(
        update={
            "leaf_schema": LeafSchema(
                format=2,
                slots=(
                    slots[0].model_copy(
                        update={
                            "name": "quantity",
                            "occurrences": tuple(
                                p for s in slots for p in s.occurrences
                            ),
                        }
                    ),
                )
                if not split_roles
                else tuple(
                    s.model_copy(update={"name": f"number_{i}"})
                    for i, s in enumerate(slots)
                ),
            )
        }
    )
    checksum = hash_payload(updated.payload(part.canonical_source))
    return updated.model_copy(
        update={"id": "frame:" + checksum, "content_hash": checksum}
    ), part.canonical_source


def test_repeated_source_values_can_share_one_authored_leaf_without_losing_units() -> (
    None
):
    raw = "自分の手札のカード２枚を選ぶ。自分の手札のカード２枚を選ぶ。"
    frame, canonical = folded(raw)
    frame.verify(canonical)
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    engine = classifier()
    found = engine.recognize(raw, field.source, part, field=field)
    matched = Frames((frame,)).match(raw, field, part, found, engine)
    assert matched is not None
    assert tuple(matched.binding.values) == ("quantity",)
    assert [
        (o.slot, o.ordinal, o.source_unit) for o in matched.binding.occurrences
    ] == [
        ("quantity", 0, "枚"),
        ("quantity", 1, "枚"),
    ]


@pytest.mark.parametrize(
    "raw",
    [
        "自分の手札のカード２枚を選ぶ。自分の手札のカード３枚を選ぶ。",
        "自分の手札のカード２枚を選ぶ。自分の手札のカード２枚まで選ぶ。",
        "自分の手札のカード２枚を選ぶ。自分の手札のカード２枚以上なら、仮。",
    ],
)
def test_equal_spelling_cannot_merge_distinct_source_values_modes_or_roles(
    raw: str,
) -> None:
    frame, _ = folded(raw)
    field = normalize_source(raw, source(raw))
    engine = classifier()
    found = engine.recognize(raw, field.source, field.parts[0], field=field)
    with pytest.raises(ValueError, match="leaves differ"):
        Frames((frame,)).match(raw, field, field.parts[0], found, engine)


def test_pending_frame_cannot_be_selected_for_another_owner_with_equal_bytes() -> None:
    raw = "自分の手札のカード２枚を選ぶ。自分の手札のカード２枚を選ぶ。"
    frame, _ = folded(raw)
    descriptor = source(raw)
    descriptor = descriptor.model_copy(
        update={
            "owner": descriptor.owner.model_copy(update={"revision_id": "other-owner"})
        }
    )
    field = normalize_source(raw, descriptor)
    engine = classifier()
    found = engine.recognize(raw, descriptor, field.parts[0], field=field)
    assert Frames((frame,)).match(raw, field, field.parts[0], found, engine) is None


def test_two_applicable_authored_slot_layouts_require_an_explicit_resolution() -> None:
    raw = "自分の手札のカード２枚を選ぶ。自分の手札のカード２枚を選ぶ。"
    shared, _ = folded(raw)
    separate, _ = folded(raw, split_roles=True)
    field = normalize_source(raw, source(raw))
    engine = classifier()
    found = engine.recognize(raw, field.source, field.parts[0], field=field)
    with pytest.raises(ValueError, match="Ambiguous authored frames"):
        Frames((shared, separate)).match(raw, field, field.parts[0], found, engine)
