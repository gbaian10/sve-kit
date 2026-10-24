"""Only the complete printed Lastword receives the uniform all-leader contract."""

from dataclasses import replace

import pytest

from sve_carddb.contracts.four_layer import GlossaryReference, PrintingFaceOwner
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_damage import KEY, prepare_damage
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_semantics import CardContext

from .test_four_layer_classification import classifier, source
from .test_four_layer_draw import ascii_id

RAW = "{ラストワード}リーダーすべてに2ダメージ。"
EXTRA = ("braced_ability_reference", "suffix_damage_amount")
TERM = Term("term:ability.last_words", "ability", "ラストワード")


def facts(raw: str = RAW) -> CardContext:
    return CardContext(
        source(raw),
        "c:synthetic",
        "f:synthetic",
        "normal",
        "follower",
        True,
        0,
        frozenset(),
    )


@pytest.mark.parametrize("count", [0, 1, 2, 9007199254740991])
def test_complete_lastword_replays_fixed_head_and_uniform_amount(count: int) -> None:
    raw = RAW.replace("2", str(count))
    context = facts(raw)
    engine = classifier((TERM,), EXTRA)
    derived = derive_source(raw, context.source, engine, context=context)
    frame, binding = derived.bind(0)
    derived.verify(raw, engine, frame, binding, context=context)
    assert frame.semantic_variant.key == KEY
    assert frame.projection.discriminator == KEY
    assert frame.projection.projection_kind == "ability_body"
    assert [s.id for s in frame.projection.scopes] == ["ability_0"]
    assert frame.projection.imports == frame.projection.exports == ()
    assert binding.values == {
        "leaf_0": GlossaryReference(kind="glossary", key=TERM.id),
        "leaf_1": count,
    }
    meaning = frame.source.semantic_definitions["resolved"]
    assert isinstance(meaning, dict)
    assert meaning["recipients"] == "all_leaders_including_self"
    assert meaning["amount"] == {"mode": "uniform", "slot": "leaf_1"}
    assert meaning["damage_kind"] == "ability"
    assert meaning["batch"] == "single_simultaneous_instruction"
    original = facts()
    same_frame, other_binding = derive_source(
        RAW, original.source, engine, context=original
    ).bind(0)
    assert frame.id == same_frame.id
    assert (binding.id == other_binding.id) == (count == 2)


@pytest.mark.parametrize(
    "raw",
    [
        RAW.replace("リーダーすべて", "相手のリーダーすべて"),
        RAW.replace("{ラストワード}", "{ファンファーレ}"),
        RAW.replace("{ラストワード}", "【進化時】"),
        RAW.replace("{ラストワード}", "【攻撃時】"),
        RAW.replace("リーダーすべて", "フォロワーすべて"),
        RAW.replace("リーダーすべてに", "リーダー1人を選ぶ。それに"),
        RAW.replace("リーダーすべてに", "リーダー2人まで選ぶ。それに"),
        RAW.replace("2", "X"),
        RAW.replace("2", "02"),
        RAW.replace("2", "２"),
        RAW.replace("2", "9007199254740992"),
        RAW.replace("ダメージ。", "ダメージを割りふる。"),
        RAW.replace("ダメージ。", "ダメージを与えてよい。"),
        RAW.replace("リーダー", "【覚醒】状態なら、リーダー"),
        RAW + "1枚引く。",
        RAW + "\n1枚引く。",
        "{クイック}\n" + RAW,
        "{コスト1}：" + RAW,
        "「" + RAW + "」を得る。",
        "（" + RAW + "）",
        RAW + "\n",
        " " + RAW,
    ],
    ids=ascii_id,
)
def test_similar_sources_do_not_gain_the_rule(raw: str) -> None:
    context = facts(raw)
    derived = derive_source(
        raw, context.source, classifier((TERM,), EXTRA), context=context
    )
    assert all(
        r.semantics is None or r.semantics.key != KEY for r in derived.recognized
    )


@pytest.mark.parametrize(
    "mutation",
    [
        "history",
        "sections",
        "unknown_sections",
        "spell",
        "amulet",
        "crest",
        "evolved",
        "advance",
        "token",
        "unknown_special",
        "field",
        "printing",
        "missing",
    ],
)
def test_source_owner_and_complete_normal_face_are_required(mutation: str) -> None:
    context = facts()
    if mutation == "history":
        context = replace(context, current=False)
    elif mutation in {"sections", "unknown_sections"}:
        context = replace(context, section_count=1 if mutation == "sections" else None)
    elif mutation in {"spell", "amulet", "crest"}:
        context = replace(context, card_kind=mutation)
    elif mutation in {"evolved", "advance"}:
        context = replace(context, phase=mutation)
    elif mutation in {"token", "unknown_special"}:
        context = replace(
            context, special_kinds=frozenset({"token"}) if mutation == "token" else None
        )
    elif mutation == "field":
        context = replace(context, source=source(RAW, "section"))
    elif mutation == "printing":
        context = replace(
            context,
            source=context.source.model_copy(
                update={
                    "owner": PrintingFaceOwner(
                        kind="printing_face",
                        printing_id="p:synthetic",
                        face_id=context.face_id,
                    ),
                }
            ),
        )
    descriptor = context.source
    derived = derive_source(
        RAW,
        descriptor,
        classifier((TERM,), EXTRA),
        context=None if mutation == "missing" else context,
    )
    assert all(
        r.semantics is None or r.semantics.key != KEY for r in derived.recognized
    )


@pytest.mark.parametrize(
    "term", [None, Term("term:ability.other", "ability", "ラストワード")]
)
def test_fixed_concept_is_not_an_arbitrary_head_parameter(term: Term | None) -> None:
    context = facts()
    derived = derive_source(
        RAW,
        context.source,
        classifier(() if term is None else (term,), EXTRA),
        context=context,
    )
    assert derived.recognized[0].semantics is None


def test_duplicate_resolution_is_rejected() -> None:
    context = facts()
    derived = derive_source(
        RAW, context.source, classifier((TERM,), EXTRA), context=context
    )
    with pytest.raises(ValueError, match="ambiguous_semantic_variant"):
        prepare_damage(RAW, derived, context)


def test_excessive_decimal_is_rejected_at_the_existing_numeric_boundary() -> None:
    raw = RAW.replace("2", "9" * 5000)
    context = facts(raw)
    with pytest.raises(ValueError, match=r"limit.*integer string conversion"):
        derive_source(raw, context.source, classifier((TERM,), EXTRA), context=context)


@pytest.mark.parametrize("mutation", ["ordinal", "different_source"])
def test_inconsistent_source_descriptor_is_rejected_before_classification(
    mutation: str,
) -> None:
    context = facts()
    if mutation == "ordinal":
        context = replace(
            context, source=context.source.model_copy(update={"ordinal": 0})
        )
    descriptor = context.source
    if mutation == "different_source":
        context = facts(RAW + "\n")
    with pytest.raises(
        ValueError, match=r"Source field and ordinal disagree|Semantic context belongs"
    ):
        derive_source(RAW, descriptor, classifier((TERM,), EXTRA), context=context)
