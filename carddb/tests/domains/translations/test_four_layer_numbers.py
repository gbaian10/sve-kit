"""Complete numeric constructions prove roles without borrowing neighboring syntax."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import Constant, QuantitySpec
from sve_carddb.domains.translations.four_layer_classification import Classifier, Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source

from .test_four_layer_classification import classifier, source, type_vocabulary

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Frame, LeafSlot
    from sve_carddb.contracts.source_binding import LeafOccurrence, SourceBinding


def numeric_slots(frame: Frame) -> tuple[LeafSlot, ...]:
    return tuple(
        s
        for s in frame.leaf_schema.slots
        if s.type in {"Nat", "Ordinal", "QuantitySpec"}
    )


def numeric_occurrences(
    frame: Frame, binding: SourceBinding
) -> tuple[LeafOccurrence, ...]:
    names = {s.name for s in numeric_slots(frame)}
    return tuple(o for o in binding.occurrences if o.slot in names)


def numeric_classifier(
    terms: tuple[Term, ...] = (), extra: tuple[str, ...] = ()
) -> Classifier:
    return classifier(terms, extra, vocabulary=type_vocabulary())


@pytest.mark.parametrize(
    ("raw", "roles"),
    [
        (
            "このターン、自分の場のフォロワーが上限解放や最終上限解放を合わせて２回以上していたなら、仮。",
            ["threshold"],
        ),
        (
            "この能力はこのターン、相手の場のフォロワーが上限解放や最終上限解放を合わせて４回以上していたなら使える。",
            ["threshold"],
        ),
        (
            "自分の墓場の{Q}{起動}能力を持つフォロワーが４枚以上なら、仮。",
            ["existence_count"],
        ),
        (
            "自分のデッキの上４枚を見る。その中から、{クイック}を持つスペルや{Q}{起動}能力を持つフォロワー２枚まで公開して手札に加えてよい。",
            ["count", "selection_count"],
        ),
        (
            "自分の墓場の{Q}{起動}能力を持つフォロワーを元のコストの合計が４以下になるように２枚まで選ぶ。",
            ["threshold", "selection_count"],
        ),
        (
            "自分のデッキから元のコスト２の{Q}{起動}能力を持つフォロワー３枚を探し、場に出す。",
            ["cost_value", "count"],
        ),
        (
            "自分のエボルヴデッキの裏向きの仮族・エボルヴフォロワー２枚まで表向きにしてよい。",
            ["selection_count"],
        ),
        (
            "相手のPP最大値が６以上である限り、これが受けるダメージを-２する。",
            ["threshold", "damage_delta_magnitude"],
        ),
    ],
)
def test_limit_break_batch_numeric_constructions(raw: str, roles: list[str]) -> None:
    engine = numeric_classifier(
        extra=(
            "original_cost_sum_bound",
            "pp_capacity_bound",
            "received_all_damage_delta",
        )
    )
    derived = derive_source(raw, source(raw), engine)
    assert not derived.recognized[0].issues
    frame, binding = derived.bind(0)
    engine.verify(raw, derived.field, derived.field.parts[0], frame, binding)
    assert [s.role for s in numeric_slots(frame)] == roles


@pytest.mark.parametrize(
    "raw",
    [
        "自分の場のフォロワーが上限解放や最終上限解放を合わせて２回以上していたなら、仮。",
        "このターン、自分の場のフォロワーが不明や最終上限解放を合わせて２回以上していたなら、仮。",
        "自分の墓場の{Q}{起動}能力を持つフォロワーが２体以上なら、仮。",
        "自分のエボルヴデッキの裏向きの仮族・エボルヴフォロワー２体まで表向きにしてよい。",
        "相手のPP最大値が６である限り、仮。",
    ],
)
def test_new_numeric_forms_require_the_counted_set_unit_and_comparator(
    raw: str,
) -> None:
    engine = numeric_classifier(extra=("pp_capacity_bound",))
    derived = derive_source(raw, source(raw), engine)
    assert derived.recognized[0].issues


@pytest.mark.parametrize(
    "raw",
    [
        "仮。相手プレイヤー１人は手札を公開する。自分はその中から２枚を選ぶ。",
        "仮。自分のデッキの上X枚を見る。その中から、仮族・フォロワー１枚を公開して手札に加える。残りを好きな順にデッキの下に置く。【覚醒】状態なら、代わりに仮族・フォロワー２枚まで公開して手札に加える。",
        "仮。自分の墓場が２枚以上で自分の他の元のコストX以下の仮族・フォロワーが場か墓場に置かれたとき、仮。",
        "仮。自分の墓場が２枚以上で自分の他の元のコストX以下の仮族・フォロワーが場から墓場に置かれたとき、仮。",
    ],
)
def test_explicit_reveal_and_conjunction_contexts_prove_the_counted_set(
    raw: str,
) -> None:
    engine = numeric_classifier(extra=("player_person_quantity",))
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert any(s.type == "QuantitySpec" for s in frame.leaf_schema.slots)


@pytest.mark.parametrize(
    "raw",
    [
        "仮。相手プレイヤー１人は墓場を公開する。自分はその中から２枚を選ぶ。",
        "仮。相手プレイヤー１人は手札を公開する。{起動}自分はその中から２枚を選ぶ。",
        "仮。相手プレイヤー１人は手札を公開する。自分はその中から２体を選ぶ。",
        "仮。自分のデッキの上X枚を見る。その中から、仮族・フォロワー１枚を公開して手札に加える。残りを好きな順に墓場に置く。【覚醒】状態なら、代わりに仮族・フォロワー２枚まで公開して手札に加える。",
        "仮。自分の墓場が２枚以上で自分の他の元のコストX以下の不明物が場か墓場に置かれたとき、仮。",
    ],
)
def test_reveal_or_conjunction_cannot_inherit_an_unknown_set(raw: str) -> None:
    engine = numeric_classifier(extra=("player_person_quantity",))
    field = normalize_source(raw, source(raw))
    assert engine.recognize(raw, field.source, field.parts[0]).issues


def test_explicit_counter_carrier_union_keeps_count_and_threshold_separate() -> None:
    raw = "仮。これか自分のEXエリアのカード２枚の情熱カウンターが３個以上なら、仮。"
    engine = numeric_classifier(extra=("named_counter_threshold",))
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert [s.role for s in numeric_slots(frame)] == ["count", "threshold"]
    assert [o.source_unit for o in numeric_occurrences(frame, binding)] == ["枚", "個"]


def test_named_union_inherits_only_the_complete_deck_look_source() -> None:
    raw = (
        "仮。自分のデッキの上X枚を見る。その中から、"
        "「これと同名を除く仮族・カード」と『仮名』それぞれ２枚まで公開して手札に加えてよい。"
    )
    engine = numeric_classifier(
        (Term("term:name.synthetic_union", "card_name", "仮名"),)
    )
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert numeric_slots(frame)[-1].role == "selection_count"
    assert numeric_occurrences(frame, binding)[-1].source_unit == "枚"
    bad = raw.replace("見る。", "不明する。")
    bad_field = normalize_source(bad, source(bad))
    assert engine.recognize(bad, bad_field.source, bad_field.parts[0]).issues


def test_sum_constraint_keeps_the_explicit_amulet_trait_and_source_zone() -> None:
    raw = "仮。自分のデッキから土の印・アミュレットを元のコストの合計がX以下になるように２枚まで探し、EXエリアに置く。"
    engine = numeric_classifier()
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert numeric_slots(frame)[0].role == "selection_count"
    assert numeric_occurrences(frame, binding)[0].source_unit == "枚"


@pytest.mark.parametrize(
    ("raw", "role"),
    [
        ("仮。このターン中にこれを含めて２枚以上プレイしていたなら、仮。", "threshold"),
        ("仮。それの{UB}能力２つを元のコストを支払わずに発動する。", "count"),
        (
            "仮。自身の場のフォロワー２体を指定し、ターンプレイヤーから順にそれを手札に戻す。",
            "selection_count",
        ),
        (
            "仮。自分のデッキから【スタートアミュレット】を持つカード２枚を場に出した状態でバトルを開始する。",
            "count",
        ),
        (
            "仮。自分のデッキから「消滅させたフォロワーと同名のフォロワー」２枚まで探し、EXエリアに置く。",
            "selection_count",
        ),
    ],
)
def test_complete_actions_and_event_predicates_prove_the_numeric_role(
    raw: str, role: str
) -> None:
    engine = numeric_classifier(extra=("suffix_unit_items",))
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    if found.issues:
        assert found.issues == ("name_source_missing_or_ambiguous",)
        assert role == "selection_count"
        slot = next(s for s in found.schema.slots if s.role == role)
        assert isinstance(found.values[slot.name], QuantitySpec)
        with pytest.raises(ValueError, match="Unresolved source leaves"):
            derived.bind(part.ordinal)
        return
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert numeric_slots(frame)[0].role in (
        {"threshold", "comparison_threshold"} if role == "threshold" else {role}
    )


@pytest.mark.parametrize(
    "raw",
    [
        "仮。このターン中にこれを含めて２枚以上不明していたなら、仮。",
        "仮。これによってカード名がそれぞれ異なる２枚のカードを不明したなら、仮。",
        "仮。それの{UB}能力２つを元のコストを支払わずに不明する。",
        "仮。自分のEXエリアのトークン・カード２体を選ぶ。",
    ],
)
def test_incomplete_event_or_execution_cannot_supply_a_numeric_role(raw: str) -> None:
    engine = numeric_classifier(extra=("suffix_unit_items",))
    field = normalize_source(raw, source(raw))
    assert engine.recognize(raw, field.source, field.parts[0]).issues


@pytest.mark.parametrize("action", ["２枚か『仮別名』３枚", "２枚ずつ"])
def test_named_creation_count_does_not_infer_card_kind(action: str) -> None:
    terms = (
        Term("term:name.synthetic_first", "card_name", "仮名"),
        Term("term:name.synthetic_second", "card_name", "仮別名"),
    )
    raw = f"仮。『仮名』{action}をEXエリアに置く。"
    if action.endswith("ずつ"):
        raw = raw.replace("ずつを", "ずつ")
    engine = numeric_classifier(terms)
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert all(s.type != "CardKind" for s in frame.leaf_schema.slots)
    assert all(s.role == "count" for s in frame.leaf_schema.slots if s.type == "Nat")


@pytest.mark.parametrize("unit", ["枚", "体"])
def test_complete_ex_token_deployment_has_its_own_unit_rule(unit: str) -> None:
    raw = f"仮。自分のEXエリアの仮族・トークン・フォロワー２{unit}まで場に出す。"
    engine = numeric_classifier()
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert numeric_slots(frame)[0].role == "selection_count"
    assert binding.values["leaf_0"] == QuantitySpec(
        mode="up_to", expr=Constant(kind="constant", value=2)
    )
    assert numeric_occurrences(frame, binding)[0].source_unit == unit


@pytest.mark.parametrize(
    "raw",
    [
        "仮。自分のEXエリアの仮族・トークン・フォロワー２枚を選ぶ。",
        "仮。自分の場の仮族・トークン・フォロワー２枚まで場に出す。",
        "仮。自分のEXエリアの仮族・トークン・フォロワー２枚まで場に不明する。",
    ],
)
def test_other_actions_cannot_borrow_the_ex_deployment_unit(raw: str) -> None:
    engine = numeric_classifier()
    field = normalize_source(raw, source(raw))
    found = engine.recognize(raw, field.source, field.parts[0])
    assert found.issues


@pytest.mark.parametrize(
    "raw",
    [
        "仮。自分の墓場のトリガーを持つフォロワー２枚選ぶ。",
        "仮。自分のデッキからの仮族・アミュレット２枚を選ぶ。",
        "仮。自身の場のフォロワー２体を選ぶ。",
    ],
)
def test_explicit_counted_np_connectors_preserve_source_evidence(raw: str) -> None:
    engine = numeric_classifier()
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert numeric_slots(frame)[0].role == "selection_count"


@pytest.mark.parametrize("malformed", [False, True])
def test_counter_carrier_count_is_distinct_from_the_removed_counter_amount(
    malformed: bool,
) -> None:
    term = Term("term:name.synthetic_carrier", "card_name", "仮器")
    raw = "仮。場の『仮器』２つのギガカウンター３個を取る:仮。"
    if malformed:
        raw = raw.replace("ギガ", "未知")
    engine = numeric_classifier(
        (term,), extra=("suffix_unit_items", "named_counter_remove")
    )
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    if malformed:
        assert found.issues
        with pytest.raises(ValueError, match="Unresolved source leaves"):
            derived.bind(part.ordinal)
        return
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert [s.role for s in frame.leaf_schema.slots if s.type == "Nat"] == [
        "count",
        "counter_amount",
    ]
    numeric = {s.name for s in frame.leaf_schema.slots if s.type == "Nat"}
    assert [o.source_unit for o in binding.occurrences if o.slot in numeric] == [
        "つ",
        "個",
    ]


@pytest.mark.parametrize(
    ("np", "unit"),
    [("自分の場の仮族・カードが", "枚"), ("相手の場のフォロワーが", "体")],
)
def test_disjunct_counts_share_one_explicit_counted_set(np: str, unit: str) -> None:
    raw = f"仮。{np}２{unit}か３{unit}なら、仮。"
    engine = numeric_classifier()
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert [s.role for s in numeric_slots(frame)] == ["existence_count"] * 2
    assert [o.source_unit for o in numeric_occurrences(frame, binding)] == [unit] * 2


@pytest.mark.parametrize(
    "raw",
    [
        "仮。【融合】トークンでないフォロワー２枚:これに融合カウンターX個を不明する。",
        "仮。相手の場のフォロワー２体を選ぶ。{起動}残りの１体にXダメージ。",
        "仮。相手の場のフォロワー２体を選ぶ。それをEXエリアに置く。残りの１体にXダメージ。",
        "仮。「自分の墓場のカードが２枚以上なら、仮」を持つ。３枚以上なら、別。",
        "仮。自分のデッキを不明し、上X枚を見る。その中から、フォロワー２枚まで場に出してよい。",
        "仮。相手の場のフォロワー２枚以上に能力ダメージを与えたとき、仮。",
        "仮。自分の場のフォロワーが２体か３枚なら、仮。",
        "仮。自分の場の不明物が２体か３体なら、仮。",
    ],
)
def test_unknown_or_mismatched_counted_constructions_cannot_bind(raw: str) -> None:
    engine = numeric_classifier()
    field = normalize_source(raw, source(raw))
    found = engine.recognize(raw, field.source, field.parts[0])
    assert found.issues
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        found.bind(field.source, field.parts[0])


@pytest.mark.parametrize(
    ("raw", "role", "unit"),
    [
        (
            "仮。【融合】トークンでないフォロワー２枚:これに融合カウンター３個を置く。",
            "counter_amount",
            "個",
        ),
        (
            "仮。相手のリーダーすべてに「自分の墓場の仮族・カード」２枚につきXダメージ。",
            "group_divisor",
            "枚",
        ),
        (
            "仮。自分の手札が２枚になるように自分の手札を捨てる。",
            "existence_count",
            "枚",
        ),
        (
            "仮。これはエボルヴデッキに２枚まで入れることができる。",
            "construction_copy_limit",
            "枚",
        ),
        ("仮。好きな数２つを指定する。", "selection_count", "つ"),
        ("仮。自分は２つ以上の選択肢をチョイスする際、仮。", "threshold", "つ"),
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
    engine = numeric_classifier(
        extra=(
            "suffix_unit_items",
            "player_person_quantity",
            "suffix_ordinal_times",
            "suffix_ordinal_turns",
            "named_counter_place",
        )
    )
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert numeric_slots(frame)[-1].role in (
        {"threshold", "comparison_threshold"} if role == "threshold" else {role}
    )
    assert numeric_occurrences(frame, binding)[-1].source_unit == unit


@pytest.mark.parametrize(
    ("raw", "role"),
    [
        ("仮。自分が手札を２枚以上捨てたとき、仮。", "threshold"),
        (
            "仮。「このターン中に自分のリーダーの体力が増加した回数」が２回以上なら、仮。",
            "threshold",
        ),
        ("仮。相手のデッキ２枚が墓場に置かれたとき、仮。", "count"),
    ],
)
def test_complete_event_counts_keep_the_written_set(raw: str, role: str) -> None:
    engine = numeric_classifier()
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert numeric_slots(frame)[-1].role in (
        {"threshold", "comparison_threshold"} if role == "threshold" else {role}
    )


@pytest.mark.parametrize("count", ["０", "２"])
def test_positioning_group_reminder_requires_a_positive_complete_divisor(
    count: str,
) -> None:
    reminder = f"（{count}体ずつ上か下か決める）"
    raw = "仮。" + reminder
    engine = numeric_classifier()
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = next(p for p in field.parts if p.source_span.role == "reminder")
    found = derived.recognized[part.ordinal]
    if count == "０":
        assert found.issues
        return
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert numeric_slots(frame)[0].role == "group_divisor"
    assert numeric_occurrences(frame, binding)[0].source_unit == "体"


@pytest.mark.parametrize(
    "raw",
    [
        "仮。自分の墓場のスペルを不明な規則により２枚まで選ぶ。",
        "仮。自分の場のフォロワー２枚につき、仮。",
        "仮。２ターン目かそれ以降なら、仮。",
    ],
)
def test_incomplete_set_and_ordinal_introductions_cannot_bind(raw: str) -> None:
    engine = numeric_classifier(
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
        (
            "仮。手札の仮族・カード２枚と別族・カード３枚と第三族・カード４枚を捨てる:仮。",
            ["count", "count", "count"],
            ["枚", "枚", "枚"],
        ),
        ("仮。ドライブチェックを２回する。", ["repeat_count"], ["回"]),
        ("仮。自分はサイコロを２回ふりなおしてよい。", ["repeat_count"], ["回"]),
        ("これを２回くり返す。", ["repeat_count"], ["回"]),
        (
            "仮。相手の場のフォロワー３体まで選ぶ。その中の１体にXダメージ。",
            ["selection_count", "selection_count"],
            ["体", "体"],
        ),
        (
            "仮。同時に自分の場にフォロワーが２体以上出たとき、出たうちの１体に{攻撃力}+X/{体力}+Xする。",
            ["threshold", "selection_count"],
            ["体", "体"],
        ),
        ("仮。誘発する自分の能力は追加で２回誘発する。", ["repeat_count"], ["回"]),
        (
            "自分の墓場のカードが２枚以上なら、仮。３枚以上なら、別。",
            ["existence_count", "existence_count"],
            ["枚", "枚"],
        ),
        (
            "仮。この能力は「このターン中に自分がプレイしたカードの枚数」が４枚以上なら、２ターンに３回使える。",
            ["threshold", "duration_count", "repeat_count"],
            ["枚", "ターン", "回"],
        ),
        ("自分のＰＰを２回復する。", ["resource_amount"], ["ＰＰ"]),
    ],
)
def test_complete_movement_frequency_and_resource_recovery(
    raw: str, roles: list[str], units: list[str]
) -> None:
    engine = numeric_classifier(
        extra=("suffix_recovery_amount", "player_person_quantity")
    )
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    field.verify(
        raw,
        (frame,),
        (binding,),
        engine.domains,
    )
    assert [
        "threshold" if s.role == "comparison_threshold" else s.role
        for s in numeric_slots(frame)
    ] == roles
    expected_units = [
        None if slot.role == "comparison_threshold" else unit
        for slot, unit in zip(numeric_slots(frame), units, strict=True)
    ]
    assert [
        o.source_unit for o in numeric_occurrences(frame, binding)
    ] == expected_units


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
    engine = numeric_classifier(extra=("suffix_unit_items",))
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = next(p for p in field.parts if p.source_span.role == "reminder")
    found = derived.recognized[part.ordinal]
    if malformed:
        assert "n0_numeric_construction_unresolved" in found.issues
        return
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert [s.role for s in numeric_slots(frame)] == [
        "duration_count",
        "repeat_count",
        "group_divisor",
        "resource_amount",
    ]
    assert [o.source_unit for o in numeric_occurrences(frame, binding)] == [
        "ターン",
        "回",
        "回",
        "つ",
    ]
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
    engine = numeric_classifier(extra=("leader_person_quantity",))
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert [o.source_unit for o in numeric_occurrences(frame, binding)] == units


def test_compound_selection_cannot_hide_an_unknown_second_np() -> None:
    raw = "仮。相手のリーダー２人か不明なフォロワー３体を選ぶ。"
    engine = numeric_classifier(extra=("leader_person_quantity",))
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
        "仮。手札の仮族・カード２枚と不明なカード３枚を捨てる:仮。",
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
    engine = numeric_classifier()
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    frame, binding = derived.bind(0)
    field.verify(
        raw,
        (frame,),
        (binding,),
        engine.domains,
    )
    assert binding.values == {
        "leaf_0": QuantitySpec(mode="up_to", expr=Constant(kind="constant", value=2))
    }
    assert numeric_slots(frame)[0].role == "repeat_count"


@pytest.mark.parametrize(
    ("raw", "roles", "units"),
    [
        (
            "仮。EPを２つ裏向きにすることで、３PPを払える。",
            ["resource_amount", "resource_amount"],
            ["つ", "PP"],
        ),
        ("これは魂カウンター２つを置く。", ["counter_amount"], ["つ"]),
        ("これは魂カウンター２つまでを置いてよい。", ["counter_amount"], ["つ"]),
        ("これは魂カウンターが２つ以上なら、仮。", ["threshold"], ["つ"]),
        ("これは魂カウンター２つにつき、仮。", ["group_divisor"], ["つ"]),
    ],
)
def test_resource_items_and_closed_counter_constructions(
    raw: str, roles: list[str], units: list[str]
) -> None:
    engine = numeric_classifier(extra=("suffix_unit_items",))
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert [
        "threshold" if s.role == "comparison_threshold" else s.role
        for s in numeric_slots(frame)
    ] == roles
    assert [o.source_unit for o in numeric_occurrences(frame, binding)] == units


@pytest.mark.parametrize(
    "raw",
    [
        "これは魂カウンター２つ以上仮。",
        "これは魂カウンター０つにつき、仮。",
        "仮。EPを２つ裏向きにすることで、３PPを払える仮。",
    ],
)
def test_unknown_counter_and_incomplete_resource_or_event_cannot_bind(raw: str) -> None:
    engine = numeric_classifier(extra=("suffix_unit_items",))
    field = normalize_source(raw, source(raw))
    found = engine.recognize(raw, field.source, field.parts[0])
    assert "n0_numeric_construction_unresolved" in found.issues
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        found.bind(field.source, field.parts[0])


def test_created_name_count_preserves_its_unit_without_inferring_a_card_kind() -> None:
    raw = "仮。『仮生成物』２体を出す。"
    engine = numeric_classifier(
        (Term("term:created.synthetic", "card_name", "仮生成物"),)
    )
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert frame.leaf_schema.slots[1].role == "count"
    assert binding.occurrences[1].source_unit == "体"
    assert all(s.type != "CardKind" for s in frame.leaf_schema.slots)


def test_full_existence_permission_does_not_lose_its_comparator_mode() -> None:
    raw = "仮。自分の墓場のフォロワー２枚以上なら使える。"
    engine = numeric_classifier()
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    part = field.parts[0]
    found = derived.recognized[part.ordinal]
    assert not found.issues
    frame, binding = derived.bind(part.ordinal)
    engine.verify(raw, field, part, frame, binding)
    assert binding.values["leaf_0"] == QuantitySpec(
        mode="at_least", expr=Constant(kind="constant", value=2)
    )


@pytest.mark.parametrize(
    ("raw", "valid"),
    [
        ("下記から３つチョイスする。【１】仮。【２】別。", False),
        ("下記から１つチョイスする。（【１】仮。）【２】別。", False),
        ("下記から１つチョイスする。『仮【１】』【２】別。", False),
    ],
)
def test_choice_indices_need_one_complete_ordered_ability_scope(
    raw: str, *, valid: bool
) -> None:
    field = normalize_source(raw, source(raw))
    engine = numeric_classifier(extra=("bracket_choice_index", "suffix_unit_items"))
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
    engine = numeric_classifier(extra=("bracket_choice_index", "suffix_unit_items"))
    derived = derive_source(raw, source(raw), engine)
    field = derived.field
    found = derived.recognized[0]
    if count == "３":
        assert found.issues
        return
    assert not found.issues
    frame, binding = derived.bind(0)
    engine.verify(raw, field, field.parts[0], frame, binding)
    assert binding.values["leaf_1"] == QuantitySpec(
        mode="up_to", expr=Constant(kind="constant", value=int(count))
    )


def test_distinct_explicit_abilities_have_independent_choice_scopes() -> None:
    raw = (
        "{ファンファーレ}下記から１つチョイスする。【１】仮。【２】別。"
        "{起動}下記から１つチョイスする。【１】他。【２】替。"
    )
    engine = numeric_classifier(extra=("bracket_choice_index", "suffix_unit_items"))
    field = normalize_source(raw, source(raw))
    found = engine.recognize(raw, field.source, field.parts[0])
    assert not found.issues
    assert [
        found.values[s.name] for s in found.schema.slots if s.role == "choice_index"
    ] == [1, 2, 1, 2]
