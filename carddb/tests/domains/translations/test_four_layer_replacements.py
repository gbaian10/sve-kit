"""A replacement preserves the complete original action's type and source unit."""

import pytest

from sve_carddb.contracts.four_layer import Constant, QuantitySpec
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source

from .test_four_layer_classification import classifier, source
from .test_four_layer_numbers import numeric_classifier


@pytest.mark.parametrize(
    ("original", "condition", "replacement", "role"),
    [
        ("１枚引く。", "【土の秘術】", "３枚。", "count"),
        (
            "１枚引く。",
            "元のコストX以上の仮族・カードを捨てたなら、",
            "３枚。",
            "count",
        ),
        (
            "１枚引く。",
            "自分の場に元のコストX以上の仮族・フォロワーがいるなら、",
            "３枚。",
            "count",
        ),
        (
            "自分のデッキからフォロワー１枚を探し、EXエリアに置く。",
            "自分の墓場のカードが７枚以上なら、",
            "３枚まで。",
            "selection_count",
        ),
    ],
)
def test_source_replacement_inherits_one_adjacent_complete_action(
    original: str, condition: str, replacement: str, role: str
) -> None:
    raw = "仮。" + original + condition + "代わりに" + replacement
    engine = numeric_classifier(
        (Term("term:ability.necrocharge", "ability", "ネクロチャージ"),),
        extra=("keyword_threshold_necrocharge",),
    )
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[0]
    assert not found.issues
    frame, binding = found.bind(field.source, part)
    engine.verify(raw, field, part, frame, binding)
    leaf = tuple(
        s
        for s in frame.leaf_schema.slots
        if s.type in {"Nat", "Ordinal", "QuantitySpec"}
    )[-1]
    assert leaf.role == role
    assert binding.values[leaf.name] == (
        QuantitySpec(mode="up_to", expr=Constant(kind="constant", value=3))
        if role == "selection_count"
        else 3
    )


@pytest.mark.parametrize(
    "raw",
    [
        "仮。１枚引く。{起動}【土の秘術】代わりに３枚。",
        "仮。１枚引く。別の文。【土の秘術】代わりに３枚。",
        "仮。１枚引く。【土の秘術】代わりに３体。",
    ],
)
def test_replacement_cannot_inherit_across_unknown_structure(raw: str) -> None:
    engine = classifier()
    field = normalize_source(raw, source(raw))
    assert engine.recognize(raw, field.source, field.parts[0]).issues
