"""Complete numeric constructions prove roles without borrowing neighboring syntax."""

import pytest

from sve_carddb.contracts.four_layer import Constant, QuantitySpec
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source

from .test_four_layer_classification import classifier, source


@pytest.mark.parametrize(
    ("np", "unit"),
    [("自分の場の仮族・カードが", "枚"), ("相手の場のフォロワーが", "体")],
)
def test_disjunct_counts_share_one_explicit_counted_set(np: str, unit: str) -> None:
    raw = f"仮。{np}２{unit}か３{unit}なら、仮。"
    engine = classifier()
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = engine.recognize(raw, field.source, part)
    assert not found.issues
    frame, binding = found.bind(field.source, part)
    engine.verify(raw, field, part, frame, binding)
    assert [s.role for s in frame.leaf_schema.slots] == ["existence_count"] * 2
    assert [o.source_unit for o in binding.occurrences] == [unit] * 2


@pytest.mark.parametrize(
    "raw",
    [
        "仮。自分の場のフォロワーが２体か３枚なら、仮。",
        "仮。自分の場の不明物が２体か３体なら、仮。",
        "仮。自分の場のフォロワーが２体。か３体なら、仮。",
    ],
)
def test_disjunct_counts_require_a_shared_np_and_legal_units(raw: str) -> None:
    engine = classifier()
    field = normalize_source(raw, source(raw))
    found = engine.recognize(raw, field.source, field.parts[0])
    assert found.issues
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        found.bind(field.source, field.parts[0])


@pytest.mark.parametrize(
    ("raw", "role", "unit"),
    [
        ("仮。自分の墓場の仮N族・フォロワー２枚を選ぶ。", "selection_count", "枚"),
        (
            "仮。相手の場の【仮状態】状態のフォロワー２体まで選ぶ。",
            "selection_count",
            "体",
        ),
        (
            "仮。自分の墓場の元のコストX以下の「仮族・フォロワーか別族・アミュレット」２枚を選ぶ。",
            "selection_count",
            "枚",
        ),
        (
            "仮。自分の墓場の{ラストワード}を持つ{仮クラス}フォロワー２枚を選ぶ。",
            "selection_count",
            "枚",
        ),
        ("仮。{進化}を持つ自分の仮族・フォロワー２体を選ぶ。", "selection_count", "体"),
        (
            "仮。自分の墓場のスペルをカード名が異なるように２枚まで選ぶ。",
            "selection_count",
            "枚",
        ),
        (
            "仮。自分の墓場のカードを元のコストの合計がX以下になるように２枚まで選ぶ。",
            "selection_count",
            "枚",
        ),
        ("仮。自分の消滅領域が２枚以上なら使える。", "existence_count", "枚"),
        (
            "仮。自分の墓場のエボルヴフォロワーが２枚以上なら、仮。",
            "existence_count",
            "枚",
        ),
        ("仮。自分の場のアミュレット２つにつき、仮。", "group_divisor", "つ"),
        ("仮。カード名２つを指定する。", "selection_count", "つ"),
        ("仮。好きな数２つを指定する。", "selection_count", "つ"),
        ("仮。相手プレイヤー２人は手札を公開する。", "selection_count", "人"),
        ("仮。自分は２つ以上の選択肢をチョイスする際、仮。", "threshold", "つ"),
        (
            "仮。このターン、２回目の自分の場のフォロワーの攻撃なら、仮。",
            "repeat_index",
            "回",
        ),
        (
            "仮。このターン、２回目の自分の場のフォロワーの進化なら、仮。",
            "repeat_index",
            "回",
        ),
        ("仮。先攻のプレイヤーなら２ターン目以降、仮。", "turn_index", "ターン"),
        ("仮。自分のターンが２ターン目かそれ以降なら、仮。", "turn_index", "ターン"),
    ],
)
def test_explicit_set_constraints_designations_and_ordinals(
    raw: str, role: str, unit: str
) -> None:
    engine = classifier(
        extra=(
            "suffix_unit_items",
            "player_person_quantity",
            "suffix_ordinal_times",
            "suffix_ordinal_turns",
        )
    )
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = engine.recognize(raw, field.source, part)
    assert not found.issues
    frame, binding = found.bind(field.source, part)
    engine.verify(raw, field, part, frame, binding)
    assert frame.leaf_schema.slots[-1].role == role
    assert binding.occurrences[-1].source_unit == unit


@pytest.mark.parametrize(
    "raw",
    [
        "仮。自分の墓場のスペルを不明な規則により２枚まで選ぶ。",
        "仮。自分の墓場のカードを元のコストの合計がX以下になった２枚まで選ぶ。",
        "仮。自分の場のフォロワー２枚につき、仮。",
        "仮。このターン、２回目の不明なフォロワーの攻撃なら、仮。",
        "仮。２ターン目かそれ以降なら、仮。",
        "仮。カード名２つを指定する仮。",
    ],
)
def test_incomplete_set_and_ordinal_introductions_cannot_bind(raw: str) -> None:
    engine = classifier(
        extra=("suffix_unit_items", "suffix_ordinal_times", "suffix_ordinal_turns")
    )
    field = normalize_source(raw, source(raw))
    found = engine.recognize(raw, field.source, field.parts[0])
    assert "n0_numeric_construction_unresolved" in found.issues
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        found.bind(field.source, field.parts[0])


@pytest.mark.parametrize(
    ("raw", "roles", "units"),
    [
        ("自分の手札のカード２枚を墓場に置く。", ["count"], ["枚"]),
        ("自分の手札のカード２枚を場に出してよい。", ["count"], ["枚"]),
        ("自分のデッキからフォロワー２枚を探し、場に出す。", ["count"], ["枚"]),
        ("自分の手札のカード２枚を公開する。", ["count"], ["枚"]),
        ("仮。自分が手札２枚を捨てたとき、仮。", ["count"], ["枚"]),
        ("仮。手札２枚まで捨てる:仮。", ["selection_count"], ["枚"]),
        ("仮。次のターンに２枚引けない。", ["count"], ["枚"]),
        ("仮。「自分の手札２枚を捨てる」を持つ。", ["count"], ["枚"]),
        (
            "仮。手札の仮族・カード２枚と別族・カード３枚と第三族・カード４枚を捨てる:仮。",
            ["count", "count", "count"],
            ["枚", "枚", "枚"],
        ),
        (
            "仮。自分のデッキから仮族・フォロワー２枚と別族・フォロワー３枚を探し、場に出す。",
            ["count", "count"],
            ["枚", "枚"],
        ),
        ("仮。自分のエボルヴデッキのカード２枚を裏向きにする。", ["count"], ["枚"]),
        ("仮。ドライブチェックを２回する。", ["repeat_count"], ["回"]),
        ("仮。自分はサイコロを２回ふりなおしてよい。", ["repeat_count"], ["回"]),
        ("仮。墓場のカード２枚を消滅:仮。", ["count"], ["枚"]),
        ("仮。自分の場のフォロワー２体を手札に戻す。", ["count"], ["体"]),
        ("これを２回くり返す。", ["repeat_count"], ["回"]),
        (
            "この能力は２ターンに３回働く。",
            ["duration_count", "repeat_count"],
            ["ターン", "回"],
        ),
        (
            "仮。この能力は自分の墓場のカードが４枚以上なら、２ターンに３回使える。",
            ["existence_count", "duration_count", "repeat_count"],
            ["枚", "ターン", "回"],
        ),
        (
            "仮。「この能力は２ターンに３回働く」を持つ。",
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


@pytest.mark.parametrize("family", ["食事", "憑依"])
@pytest.mark.parametrize("malformed", [False, True])
def test_entry_reminder_numbers_require_the_entire_shared_limit_grammar(
    family: str, malformed: bool
) -> None:
    reminder = (
        f"（２ターンに進化か{family}はどちらか３回できる。４回につき使えるEPは５つ）"
    )
    if malformed:
        reminder = reminder.replace("できる", "でき仮")
    raw = "仮。" + reminder
    engine = classifier(extra=("suffix_unit_items",))
    field = normalize_source(raw, source(raw), reminders=frozenset({reminder}))
    part = next(p for p in field.parts if p.source_span.role == "reminder")
    found = engine.recognize(raw, field.source, part, field=field)
    if malformed:
        assert "n0_numeric_construction_unresolved" in found.issues
        return
    assert not found.issues
    frame, binding = found.bind(field.source, part)
    engine.verify(raw, field, part, frame, binding)
    assert [s.role for s in frame.leaf_schema.slots] == [
        "duration_count",
        "repeat_count",
        "group_divisor",
        "resource_amount",
    ]
    assert [o.source_unit for o in binding.occurrences] == ["ターン", "回", "回", "つ"]
    assert frame.semantic_variant.state == "pending"


@pytest.mark.parametrize(
    ("raw", "units"),
    [
        ("仮。相手のリーダー２人か相手の場のフォロワー３体を選ぶ。", ["人", "体"]),
        (
            "仮。相手の場のフォロワー２体と自分の墓場の元のコスト３の仮族・フォロワー４枚を選ぶ。",
            ["体", None, "枚"],
        ),
        (
            "仮。自分の墓場の仮族・フォロワー２枚か別族・フォロワー３枚を選ぶ。",
            ["枚", "枚"],
        ),
        (
            "仮。自分のデッキの上２枚を見る。その中から、スペルかアミュレット３枚を公開して手札に加えてよい。",
            ["枚", "枚"],
        ),
    ],
)
def test_compound_selections_prove_each_source_counted_set(
    raw: str, units: list[str | None]
) -> None:
    engine = classifier(extra=("leader_person_quantity",))
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = engine.recognize(raw, field.source, part)
    assert not found.issues
    frame, binding = found.bind(field.source, part)
    engine.verify(raw, field, part, frame, binding)
    assert [o.source_unit for o in binding.occurrences] == units


def test_compound_selection_cannot_hide_an_unknown_second_np() -> None:
    raw = "仮。相手のリーダー２人か不明なフォロワー３体を選ぶ。"
    engine = classifier(extra=("leader_person_quantity",))
    field = normalize_source(raw, source(raw))
    assert engine.recognize(raw, field.source, field.parts[0]).issues


def test_a_separate_clause_cannot_inherit_a_counted_collection() -> None:
    raw = "仮。自分の墓場のフォロワー２枚を選ぶ。仮族・フォロワー３枚を選ぶ。"
    field = normalize_source(raw, source(raw))
    assert classifier().recognize(raw, field.source, field.parts[0]).issues


@pytest.mark.parametrize(
    "raw",
    [
        "自分の手札のカード２枚を墓場に置る。",
        "自分の手札のカード２枚を場に出く。",
        "自分の手札のカード２枚を手札に加えす。",
        "自分の手札のカード２枚を公開して手札に加え仮。",
        "自分のデッキからフォロワー２枚を探し、場に出る。",
        "自分の手札のカード２枚を公開する仮。",
        "仮。手札の仮族・カード２枚と不明なカード３枚を捨てる:仮。",
        "仮２回行う。",
        "仮、２ターンに３回。",
        "この能力は２ターンに３回復する。",
        "この能力は２ターンに３回仮。",
        "この能力は不明なカードが４枚以上なら、２ターンに３回使える。",
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


def test_created_name_count_preserves_its_unit_without_inferring_a_card_kind() -> None:
    raw = "仮。『仮生成物』２体を出す。"
    engine = classifier((Term("term:created.synthetic", "card_name", "仮生成物"),))
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = engine.recognize(raw, field.source, part)
    assert not found.issues
    frame, binding = found.bind(field.source, part)
    engine.verify(raw, field, part, frame, binding)
    assert frame.leaf_schema.slots[1].role == "count"
    assert binding.occurrences[1].source_unit == "体"
    assert all(s.type != "CardKind" for s in frame.leaf_schema.slots)


def test_full_existence_permission_does_not_lose_its_comparator_mode() -> None:
    raw = "仮。自分の墓場のフォロワー２枚以上なら使える。"
    engine = classifier()
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = engine.recognize(raw, field.source, part)
    assert not found.issues
    frame, binding = found.bind(field.source, part)
    engine.verify(raw, field, part, frame, binding)
    assert binding.values["leaf_0"] == QuantitySpec(
        mode="at_least", expr=Constant(kind="constant", value=2)
    )


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


@pytest.mark.parametrize("count", ["１", "２", "３"])
def test_choice_replacement_uses_the_original_options_and_preserves_up_to(
    count: str,
) -> None:
    raw = (
        f"下記から１つチョイスする。仮なら、代わりに{count}つまで。【１】仮。【２】別。"
    )
    engine = classifier(extra=("bracket_choice_index", "suffix_unit_items"))
    field = normalize_source(raw, source(raw))
    found = engine.recognize(raw, field.source, field.parts[0])
    if count == "３":
        assert found.issues
        return
    assert not found.issues
    frame, binding = found.bind(field.source, field.parts[0])
    engine.verify(raw, field, field.parts[0], frame, binding)
    assert binding.values["leaf_1"] == QuantitySpec(
        mode="up_to", expr=Constant(kind="constant", value=int(count))
    )


def test_distinct_explicit_abilities_have_independent_choice_scopes() -> None:
    raw = (
        "{ファンファーレ}下記から１つチョイスする。【１】仮。【２】別。"
        "{起動}下記から１つチョイスする。【１】他。【２】替。"
    )
    engine = classifier(extra=("bracket_choice_index", "suffix_unit_items"))
    field = normalize_source(raw, source(raw))
    found = engine.recognize(raw, field.source, field.parts[0])
    assert not found.issues
    assert [
        found.values[s.name] for s in found.schema.slots if s.role == "choice_index"
    ] == [1, 2, 1, 2]
