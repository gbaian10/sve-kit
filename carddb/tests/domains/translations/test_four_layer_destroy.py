"""The admitted Advance source retains its complete Fanfare and siblings."""

from dataclasses import replace

import pytest

from sve_carddb.contracts.four_layer import GlossaryReference, PrintingFaceOwner
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_destroy import KEY, prepare_destroy
from sve_carddb.domains.translations.four_layer_semantics import CardContext

from .test_four_layer_classification import classifier, source
from .test_four_layer_draw import ascii_id

RAW = "【守護】【オーラ】\n{ファンファーレ}相手の場のフォロワーすべてを破壊する。"
TERMS = (
    Term("term:ability.fanfare", "ability", "ファンファーレ"),
    Term("term:keyword.ward", "keyword", "守護"),
    Term("term:keyword.aura", "keyword", "オーラ"),
)
EXTRA = ("braced_ability_reference", "bracket_keyword_reference")


def facts(raw: str = RAW) -> CardContext:
    return CardContext(
        source(raw),
        "c:synthetic",
        "f:synthetic",
        "advance",
        "follower",
        True,
        0,
        frozenset({"advance"}),
    )


def test_complete_source_preserves_nonbody_obligations_and_fixed_fanfare() -> None:
    context = facts()
    engine = classifier(TERMS, EXTRA)
    derived = derive_source(RAW, context.source, engine, context=context)
    frames = []
    for ordinal in range(len(derived.recognized)):
        frame, binding = derived.bind(ordinal)
        derived.verify(RAW, engine, frame, binding, context=context)
        frames.append(frame)
    assert [f.semantic_variant.key for f in frames] == [
        "card_keywords.v2",
        "metadata.layout.v1",
        KEY,
    ]
    assert frames[0].projection.projection_kind == "card_field"
    assert frames[0].projection.scopes == ()
    frame, binding = derived.bind(2)
    assert frame.projection.projection_kind == "ability_body"
    assert [s.id for s in frame.projection.scopes] == ["ability_0"]
    assert frame.projection.imports == frame.projection.exports == ()
    assert binding.values == {
        "leaf_0": GlossaryReference(kind="glossary", key="term:ability.fanfare"),
    }
    meaning = frame.source.semantic_definitions["resolved"]
    assert isinstance(meaning, dict)
    assert meaning["kind"] == "destroy"
    assert meaning["selection"] == "all_at_resolution_without_target"
    assert meaning["recipients"] == {"who": "opp", "zone": "field", "kind": "follower"}
    assert meaning["params"] == {}


@pytest.mark.parametrize(
    "raw",
    [
        RAW.replace("ファンファーレ", "ラストワード"),
        RAW.replace("相手", "自分"),
        RAW.replace("相手の場", "場"),
        RAW.replace("フォロワー", "カード"),
        RAW.replace("すべてを破壊する。", "1体を選ぶ。それを破壊する。"),
        RAW.replace("すべてを破壊する。", "1体まで選ぶ。それを破壊する。"),
        RAW.replace("破壊", "消滅"),
        RAW.replace("破壊する", "墓場に置く"),
        RAW + "1枚引く。",
        RAW + "\n1枚引く。",
        RAW.replace("破壊する。", "破壊してよい。"),
        RAW.replace("相手", "【覚醒】状態なら、相手"),
        RAW.replace("相手", "{コスト1}：相手"),
        RAW.replace("【守護】【オーラ】\n", ""),
        RAW.replace("【オーラ】", ""),
        RAW + "\n",
        " " + RAW,
    ],
    ids=ascii_id,
)
def test_other_complete_sources_remain_outside_the_rule(raw: str) -> None:
    context = facts(raw)
    derived = derive_source(
        raw, context.source, classifier(TERMS, EXTRA), context=context
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
        "normal",
        "evolved",
        "token",
        "unknown_special",
        "field",
        "printing",
        "missing",
    ],
)
def test_only_current_advance_face_effect_is_admitted(mutation: str) -> None:
    context = facts()
    if mutation == "history":
        context = replace(context, current=False)
    elif mutation in {"sections", "unknown_sections"}:
        context = replace(context, section_count=1 if mutation == "sections" else None)
    elif mutation in {"spell", "amulet", "crest"}:
        context = replace(context, card_kind=mutation)
    elif mutation in {"normal", "evolved"}:
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
    derived = derive_source(
        RAW,
        context.source,
        classifier(TERMS, EXTRA),
        context=None if mutation == "missing" else context,
    )
    assert all(
        r.semantics is None or r.semantics.key != KEY for r in derived.recognized
    )


@pytest.mark.parametrize(
    "term", [None, Term("term:ability.other", "ability", "ファンファーレ")]
)
def test_fixed_head_requires_exact_typed_concept(term: Term | None) -> None:
    context = facts()
    terms = TERMS[1:] if term is None else (term, *TERMS[1:])
    derived = derive_source(
        RAW, context.source, classifier(terms, EXTRA), context=context
    )
    assert derived.recognized[2].semantics is None


def test_conflicting_resolution_is_rejected() -> None:
    context = facts()
    derived = derive_source(
        RAW, context.source, classifier(TERMS, EXTRA), context=context
    )
    with pytest.raises(ValueError, match="ambiguous_semantic_variant"):
        prepare_destroy(RAW, derived, context)


@pytest.mark.parametrize("missing", ["term:keyword.ward", "term:keyword.aura"])
def test_incomplete_keyword_sibling_prevents_fanfare_adoption(missing: str) -> None:
    context = facts()
    engine = classifier(tuple(t for t in TERMS if t.id != missing), EXTRA)
    derived = derive_source(RAW, context.source, engine, context=context)
    assert derived.recognized[2].semantics is None
