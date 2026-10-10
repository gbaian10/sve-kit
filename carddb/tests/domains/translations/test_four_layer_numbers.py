"""Complete numeric constructions prove roles without borrowing neighboring syntax."""

import pytest

from sve_carddb.contracts.four_layer import Constant, QuantitySpec
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source

from .test_four_layer_classification import classifier, source


@pytest.mark.parametrize(
    ("raw", "roles", "units"),
    [
        ("自分の手札のカード２枚を墓場に置く。", ["count"], ["枚"]),
        ("自分の手札のカード２枚を場に出してよい。", ["count"], ["枚"]),
        ("これを２回くり返す。", ["repeat_count"], ["回"]),
        (
            "この能力は２ターンに３回働く。",
            ["duration_count", "repeat_count"],
            ["ターン", "回"],
        ),
        ("自分のＰＰを２回復する。", ["resource_amount"], ["ＰＰ"]),
        ("自分のPPを２回復する。", ["resource_amount"], ["PP"]),
    ],
)
def test_complete_movement_frequency_and_resource_recovery(
    raw: str, roles: list[str], units: list[str]
) -> None:
    engine = classifier(extra=("suffix_recovery_amount",))
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = engine.recognize(raw, field.source, part)
    assert not found.issues
    frame, binding = found.bind(field.source, part)
    field.verify(raw, (frame,), (binding,), engine.domains)
    assert [s.role for s in frame.leaf_schema.slots] == roles
    assert [o.source_unit for o in binding.occurrences] == units


@pytest.mark.parametrize(
    "raw",
    [
        "自分の手札のカード２枚を墓場に置る。",
        "自分の手札のカード２枚を場に出く。",
        "自分の手札のカード２枚を手札に加えす。",
        "自分の手札のカード２枚を公開して手札に加え仮。",
        "仮２回行う。",
        "仮、２ターンに３回。",
        "この能力は２ターンに３回復する。",
        "この能力は２ターンに３回仮。",
    ],
)
def test_malformed_movement_and_incomplete_frequency_cannot_bind(raw: str) -> None:
    field = normalize_source(raw, source(raw))
    found = classifier().recognize(raw, field.source, field.parts[0])
    assert "n0_numeric_construction_unresolved" in found.issues
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        found.bind(field.source, field.parts[0])


def test_repeat_up_to_retains_mode_without_turning_recovery_into_repetition() -> None:
    raw = "これを２回まで行う。"
    field = normalize_source(raw, source(raw))
    engine = classifier()
    frame, binding = engine.recognize(raw, field.source, field.parts[0]).bind(
        field.source, field.parts[0]
    )
    field.verify(raw, (frame,), (binding,), engine.domains)
    assert binding.values == {
        "leaf_0": QuantitySpec(mode="up_to", expr=Constant(kind="constant", value=2))
    }
    assert frame.leaf_schema.slots[0].role == "repeat_count"


@pytest.mark.parametrize(
    ("raw", "roles", "units"),
    [
        ("仮。SEPを２つ持つ。", ["resource_amount"], ["つ"]),
        (
            "仮。EPを２つ裏向きにすることで、３PPを払える。",
            ["resource_amount", "resource_amount"],
            ["つ", "PP"],
        ),
        ("これは魂カウンター２つを置く。", ["counter_amount"], ["つ"]),
        ("これは魂カウンター２つまでを置いてよい。", ["counter_amount"], ["つ"]),
        ("これは魂カウンターが２つ以上なら、仮。", ["threshold"], ["つ"]),
        ("これは魂カウンター２つにつき、仮。", ["group_divisor"], ["つ"]),
        ("このターン、仮の動作が２回以上発動していたなら、仮。", ["threshold"], ["回"]),
    ],
)
def test_resource_items_and_closed_counter_constructions(
    raw: str, roles: list[str], units: list[str]
) -> None:
    engine = classifier(extra=("suffix_unit_items",))
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = engine.recognize(raw, field.source, part)
    assert not found.issues
    frame, binding = found.bind(field.source, part)
    engine.verify(raw, field, part, frame, binding)
    assert [s.role for s in frame.leaf_schema.slots] == roles
    assert [o.source_unit for o in binding.occurrences] == units


@pytest.mark.parametrize(
    "raw",
    [
        "これは未知カウンター２つを置く。",
        "これは魂カウンター２つを置る。",
        "これは魂カウンター２つ以上仮。",
        "これは魂カウンター０つにつき、仮。",
        "仮。EPを２つ裏向きにすることで、３PPを払える仮。",
        "このターン、仮の動作が２回以上発動していた仮。",
    ],
)
def test_unknown_counter_and_incomplete_resource_or_event_cannot_bind(raw: str) -> None:
    engine = classifier(extra=("suffix_unit_items",))
    field = normalize_source(raw, source(raw))
    found = engine.recognize(raw, field.source, field.parts[0])
    assert "n0_numeric_construction_unresolved" in found.issues
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        found.bind(field.source, field.parts[0])


@pytest.mark.parametrize(
    ("raw", "valid"),
    [
        ("下記から１つチョイスする。【１】仮。【２】別。", True),
        ("下記から１つチョイスする。\n【１】仮。\n【２】別。", True),
        ("下記から１つチョイスする。【２】仮。【１】別。", False),
        ("下記から３つチョイスする。【１】仮。【２】別。", False),
        ("下記から１つチョイスする。【１】仮。\n\n【２】別。", False),
        (
            "下記から１つチョイスする。【１】仮。{ファンファーレ}【２】別。",
            False,
        ),
        ("下記から１つチョイスする。（【１】仮。）【２】別。", False),
        ("下記から１つチョイスする。『仮【１】』【２】別。", False),
        (
            "下記から１つチョイスする。【１】仮。【２】別。下記から１つチョイスする。",
            False,
        ),
    ],
)
def test_choice_indices_need_one_complete_ordered_ability_scope(
    raw: str, *, valid: bool
) -> None:
    field = normalize_source(raw, source(raw))
    engine = classifier(extra=("bracket_choice_index", "suffix_unit_items"))
    found = tuple(engine.recognize(raw, field.source, p) for p in field.parts)
    ordinals = tuple(
        (s, f.values[s.name])
        for f in found
        for s in f.schema.slots
        if s.role == "choice_index"
    )
    assert bool(ordinals) is valid
    if valid:
        assert [value for _, value in ordinals] == [1, 2]
        assert all(not f.issues for f in found)
    else:
        assert any(f.issues for f in found)
