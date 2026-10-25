"""Closed search, buff and token movement preserve their source boundaries."""

from dataclasses import replace

import pytest

from sve_carddb.contracts.four_layer import Constant, QuantitySpec
from sve_carddb.domains.translations.four_layer_classification import Classifier, Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_fixed_actions import (
    MOVE,
    NUMERIC,
    SEARCH,
    prepare_fixed_actions,
)
from sve_carddb.domains.translations.four_layer_semantics import CardContext

from .test_four_layer_classification import classifier, source
from .test_four_layer_draw import ascii_id

SEARCH_RAW = "{ラストワード}自分のデッキから『式神の使役』1枚を探し、手札に加える。"
NUMERIC_RAW = "{ファンファーレ}自分の他のフォロワーすべては{攻撃力}+1/{体力}+1する。"
MOVE_RAW = "これが場を離れたとき、1枚引く。自分の手札1枚を捨てる。\n{起動}{コスト1}：これを墓場に置く。"
TERMS = (
    Term("term:ability.last_words", "ability", "ラストワード"),
    Term("term:ability.fanfare", "ability", "ファンファーレ"),
    Term("term:ability.activation", "ability", "起動"),
    Term("term:stat.attack", "rule_term", "攻撃力"),
    Term("term:stat.health", "rule_term", "体力"),
    Term("term:name.shikigami_employment", "card_name", "式神の使役"),
)
EXTRA = (
    "braced_ability_reference",
    "braced_stat_reference",
    "prefix_attack_delta",
    "prefix_health_delta",
)
CASES = ((SEARCH_RAW, SEARCH, 0), (NUMERIC_RAW, NUMERIC, 0), (MOVE_RAW, MOVE, 2))


def facts(raw: str, key: str) -> CardContext:
    return CardContext(
        source(raw),
        "c:synthetic",
        "f:synthetic",
        "normal",
        "amulet" if key == MOVE else "follower",
        True,
        0,
        frozenset({"token"}) if key == MOVE else frozenset(),
    )


@pytest.mark.parametrize(("raw", "key", "ordinal"), CASES, ids=ascii_id)
def test_complete_source_and_sealed_meaning(raw: str, key: str, ordinal: int) -> None:
    context = facts(raw, key)
    engine = classifier(TERMS, EXTRA)
    derived = derive_source(raw, context.source, engine, context=context)
    frame, binding = derived.bind(ordinal)
    derived.verify(raw, engine, frame, binding, context=context)
    assert frame.semantic_variant.key == key
    assert frame.projection.projection_kind == "ability_body"
    assert [s.id for s in frame.projection.scopes] == ["ability_0"]
    assert frame.projection.imports == frame.projection.exports == ()
    meaning = frame.source.semantic_definitions["resolved"]
    assert isinstance(meaning, dict)
    if key == SEARCH:
        assert binding.values["leaf_4"] == QuantitySpec(
            mode="exact", expr=Constant(kind="constant", value=1)
        )
        assert meaning["params"] == {"n": "leaf_4.constant"}
    elif key == NUMERIC:
        assert meaning["operation"] == "add"
        assert meaning["power"] == meaning["hp"] == 1
        assert meaning["recipients"] == {
            "who": "self",
            "zone": "field",
            "kind": "follower",
            "other": True,
        }
        assert meaning["params"] == {}
    else:
        assert meaning["cost"] == {"pp": 1, "payer": "ability_master"}
        assert meaning["destination"] == "card_owner_cemetery"
        assert meaning["destroy"] is False
        assert derived.recognized[0].semantics is None
        assert derived.recognized[1].semantics is not None


@pytest.mark.parametrize("count", [0, 2, 9007199254740991])
def test_search_only_count_varies_with_identical_frame_identity(count: int) -> None:
    engine = classifier(TERMS, EXTRA)
    frames = []
    for n in (1, count):
        raw = SEARCH_RAW.replace("1枚", f"{n}枚")
        context = facts(raw, SEARCH)
        derived = derive_source(raw, context.source, engine, context=context)
        frame, binding = derived.bind(0)
        derived.verify(raw, engine, frame, binding, context=context)
        assert frame.semantic_variant.key == SEARCH
        frames.append(frame.id)
    assert frames[0] == frames[1]


@pytest.mark.parametrize(
    ("raw", "key"),
    [
        (SEARCH_RAW.replace("1枚", "01枚"), SEARCH),
        (SEARCH_RAW.replace("1枚", "9007199254740992枚"), SEARCH),
        (SEARCH_RAW.replace("1枚", "1枚まで"), SEARCH),
        (SEARCH_RAW.replace("自分", "相手"), SEARCH),
        (SEARCH_RAW.replace("デッキ", "墓場"), SEARCH),
        (SEARCH_RAW.replace("手札", "場"), SEARCH),
        (SEARCH_RAW.replace("式神の使役", "別の名前"), SEARCH),
        (SEARCH_RAW.replace("ラストワード", "ファンファーレ"), SEARCH),
        (NUMERIC_RAW.replace("+1", "+2"), NUMERIC),
        (NUMERIC_RAW.replace("他の", ""), NUMERIC),
        (NUMERIC_RAW.replace("自分", "相手"), NUMERIC),
        (NUMERIC_RAW.replace("すべて", "1体"), NUMERIC),
        (NUMERIC_RAW.replace("する。", "する。このターン中。"), NUMERIC),
        (MOVE_RAW.replace("コスト1", "コスト2"), MOVE),
        (MOVE_RAW.replace("墓場に置く", "破壊する"), MOVE),
        (MOVE_RAW.replace("墓場", "手札"), MOVE),
        (MOVE_RAW.replace("これを墓場", "これ以外を墓場"), MOVE),
        (MOVE_RAW.replace("{起動}", "{クイック}{起動}"), MOVE),
        (MOVE_RAW.split("\n")[1], MOVE),
        (MOVE_RAW.replace("1枚引く", "2枚引く"), MOVE),
        *[
            (raw + tail, key)
            for raw, key, _ in CASES
            for tail in ("1枚引く。", "\n1枚引く。", "\n")
        ],
        *[("「" + raw + "」", key) for raw, key, _ in CASES],
    ],
    ids=ascii_id,
)
def test_related_sources_do_not_gain_admission(raw: str, key: str) -> None:
    context = facts(raw, key)
    derived = derive_source(
        raw, context.source, classifier(TERMS, EXTRA), context=context
    )
    assert all(
        r.semantics is None or r.semantics.key != key for r in derived.recognized
    )


@pytest.mark.parametrize(("raw", "key", "ordinal"), CASES, ids=ascii_id)
@pytest.mark.parametrize(
    "mutation",
    [
        "history",
        "section",
        "unknown_sections",
        "unknown_special",
        "wrong_special",
        "evolved",
        "wrong_kind",
        "field",
        "missing",
    ],
)
def test_exact_current_holder_context_is_required(
    raw: str, key: str, ordinal: int, mutation: str
) -> None:
    context = facts(raw, key)
    if mutation == "history":
        context = replace(context, current=False)
    elif mutation in {"section", "unknown_sections"}:
        context = replace(context, section_count=1 if mutation == "section" else None)
    elif mutation in {"wrong_special", "unknown_special"}:
        context = replace(
            context,
            special_kinds=frozenset({"advance"})
            if mutation == "wrong_special"
            else None,
        )
    elif mutation == "evolved":
        context = replace(context, phase="evolved")
    elif mutation == "wrong_kind":
        context = replace(context, card_kind="spell")
    elif mutation == "field":
        context = replace(context, source=source(raw, "section"))
    derived = derive_source(
        raw,
        context.source,
        classifier(TERMS, EXTRA),
        context=None if mutation == "missing" else context,
    )
    assert derived.recognized[ordinal].semantics is None


@pytest.mark.parametrize(("raw", "key", "ordinal"), CASES, ids=ascii_id)
def test_conflicting_resolution_refuses_order_based_choice(
    raw: str, key: str, ordinal: int
) -> None:
    context = facts(raw, key)
    derived = derive_source(
        raw, context.source, classifier(TERMS, EXTRA), context=context
    )
    assert derived.recognized[ordinal].semantics is not None
    with pytest.raises(ValueError, match="ambiguous_semantic_variant"):
        prepare_fixed_actions(raw, derived, context)


@pytest.mark.parametrize("missing", [t.id for t in TERMS])
def test_fixed_concepts_and_name_require_exact_registered_members(missing: str) -> None:
    raw, key, ordinal = CASES[
        2
        if missing.endswith("activation")
        else 0
        if missing.endswith(("last_words", "shikigami_employment"))
        else 1
    ]
    context = facts(raw, key)
    derived = derive_source(
        raw,
        context.source,
        classifier(tuple(t for t in TERMS if t.id != missing), EXTRA),
        context=context,
    )
    assert derived.recognized[ordinal].semantics is None


def test_unrecognized_sibling_cannot_be_hidden_by_a_recognized_move() -> None:
    original = classifier(TERMS, EXTRA)
    rules = original.rules.model_copy(
        update={
            "rules": tuple(
                rule
                for rule in original.rules.rules
                if rule.rule_id != "suffix_unit_cards"
            )
        }
    )
    engine = Classifier(TERMS, original.references, rules)
    context = facts(MOVE_RAW, MOVE)
    derived = derive_source(MOVE_RAW, context.source, engine, context=context)
    assert derived.recognized[2].issues == ()
    assert derived.recognized[2].semantics is None
