"""Mandatory selection notes require the same owner's exact body and count."""

from dataclasses import replace

import pytest

from sve_carddb.contracts.four_layer import Constant, QuantitySpec
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source

from .test_four_layer_classification import classifier, source


@pytest.mark.parametrize(
    ("body", "valid"),
    [
        ("自分の墓場の仮族・カード２枚を選ぶ。それを消滅させる。", True),
        ("自分の墓場の仮族・カード３枚を選ぶ。それを消滅させる。", False),
        ("自分の墓場の仮族・カード２枚まで選ぶ。", False),
        ("自分の場のフォロワー２体を選ぶ。", False),
        ("自分の墓場の仮族・カード２枚を不明する。", False),
        ("自分の墓場の仮族・カード２枚を選ぶ。相手の手札のカード２枚を選ぶ。", False),
        ("自分の墓場の仮族・カード２枚を選ぶ。\n別。", False),
    ],
)
def test_mandatory_selection_note_replays_one_exact_body_count(
    body: str, valid: bool
) -> None:
    note = "（２枚を選べなければプレイできない）"
    raw = body + note
    field = normalize_source(raw, source(raw), reminders=frozenset({note}))
    part = field.parts[-1]
    assert part.source_span.role == "reminder"
    engine = classifier()
    found = engine.recognize(raw, field.source, part, field=field)
    assert bool(found.issues) is not valid
    if valid:
        frame, binding = found.bind(field.source, part)
        engine.verify(raw, field, part, frame, binding)
        assert frame.leaf_schema.slots[0].role == "selection_count"
        assert binding.values["leaf_0"] == QuantitySpec(
            mode="exact", expr=Constant(kind="constant", value=2)
        )
        assert binding.occurrences[0].source_unit == "枚"
        assert engine.recognize(raw, field.source, part).issues


def test_mandatory_note_cannot_use_a_field_from_another_owner() -> None:
    note = "（２枚を選べなければプレイできない）"
    raw = "自分の墓場の仮族・カード２枚を選ぶ。" + note
    field = normalize_source(raw, source(raw), reminders=frozenset({note}))
    foreign = replace(field, source=field.source.model_copy(update={"ordinal": 1}))
    with pytest.raises(ValueError, match="another exact owner"):
        classifier().recognize(raw, field.source, field.parts[-1], field=foreign)
