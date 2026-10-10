"""Whole construction and owner context decide resolution independently of targets."""

from dataclasses import replace

import pytest

from sve_carddb.contracts.four_layer import Frame
from sve_carddb.core.json import canonical
from sve_carddb.domains.catalog.models import Term as VocabularyTerm
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.vocabulary import Binding, Vocabulary
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.four_layer_semantics import CardContext

from .test_four_layer_classification import classifier, source


def context(raw: str) -> CardContext:
    return CardContext(
        source(raw),
        "card:synthetic",
        "face:normal",
        "normal",
        "follower",
    )


@pytest.mark.parametrize("separator", ["", "の", "・"])
def test_class_filter_accepts_complete_direct_kind_constructions(
    separator: str,
) -> None:
    raw = f"仮。自分の墓場の{{仮クラス}}{separator}フォロワー２枚を選ぶ。"
    field = normalize_source(raw, source(raw))
    engine = classifier()
    engine.references.vocabulary = Vocabulary(
        bindings=(
            Binding(region="jp", kind="class", raw="仮クラス", code="synthetic"),
        ),
        terms=(
            VocabularyTerm(
                kind="class",
                code="synthetic",
                label=LocalizedText(lang="ja", text="仮クラス"),
            ),
        ),
    )
    part = field.parts[0]
    found = engine.recognize(raw, field.source, part)
    assert not found.issues
    frame, binding = found.bind(field.source, part)
    engine.verify(raw, field, part, frame, binding)
    assert frame.leaf_schema.slots[0].role == "class_filter"


def test_class_marker_without_a_complete_kind_filter_stays_unresolved() -> None:
    raw = "仮。{仮クラス}未知。"
    field = normalize_source(raw, source(raw))
    engine = classifier()
    engine.references.vocabulary = Vocabulary(
        bindings=(
            Binding(region="jp", kind="class", raw="仮クラス", code="synthetic"),
        ),
        terms=(
            VocabularyTerm(
                kind="class",
                code="synthetic",
                label=LocalizedText(lang="ja", text="仮クラス"),
            ),
        ),
    )
    found = engine.recognize(raw, field.source, field.parts[0])
    assert "n0_vocabulary_construction_unresolved" in found.issues


@pytest.mark.parametrize(
    ("raw", "key", "scope_count"),
    [
        ("{進化}{コスト９９}:これは進化する。", "evolve_entry.v1", 1),
        ("{食事}{コスト９９}:これは出走する。", "feed_entry.v1", 1),
        ("{憑依}{コスト９９}:これはドライブを持つ。", "ride_entry.v1", 1),
        (
            "{進化}{コスト９９}:これは進化する。{食事}{コスト９８}:これは出走する。",
            "evolve_feed_entries.v1",
            2,
        ),
        (
            "{進化}{コスト９９}:これは進化する。{憑依}{コスト９８}:これはドライブを持つ。",
            "evolve_ride_entries.v1",
            2,
        ),
    ],
)
def test_entry_families_have_distinct_semantics_and_independent_scopes(
    raw: str, key: str, scope_count: int
) -> None:
    engine = classifier(
        (
            Term("term:ability.feed", "ability", "食事"),
            Term("term:ability.possession", "ability", "憑依"),
        ),
        ("braced_ability_reference",),
    )
    field = normalize_source(raw, source(raw))
    found = engine.recognize(
        raw, field.source, field.parts[0], context=context(raw), field=field
    )
    frame, binding = found.bind(field.source, field.parts[0])
    field.verify(raw, (frame,), (binding,), engine.domains)
    assert frame.semantic_variant.key == key
    assert frame.projection.projection_kind == "ability_body"
    assert len(frame.projection.scopes) == scope_count
    assert frame.projection.imports == frame.projection.exports == ()


@pytest.mark.parametrize(
    ("phase", "kind"),
    [
        ("evolved", "follower"),
        ("advance", "follower"),
        ("normal", "spell"),
    ],
)
def test_entry_with_missing_or_wrong_face_context_stays_pending(
    phase: str,
    kind: str,
) -> None:
    raw = "{進化}{コスト９９}:これは進化する。"
    field = normalize_source(raw, source(raw))
    facts = replace(context(raw), phase=phase, card_kind=kind)
    found = classifier().recognize(raw, field.source, field.parts[0], context=facts)
    assert found.semantics is None


@pytest.mark.parametrize(
    "raw",
    [
        "{進化}{コスト９９}:これは進化する。仮。",
        "条件なら、{進化}{コスト９９}:これは進化する。",
        "{進化}{コスト９９}:これ以外を進化する。",
        "{進化}{コスト９９}:これは仮に進化する。",
    ],
)
def test_conditional_extra_or_different_actor_entry_stays_pending(raw: str) -> None:
    field = normalize_source(raw, source(raw))
    found = classifier().recognize(
        raw, field.source, field.parts[0], context=context(raw)
    )
    assert found.semantics is None


def test_equal_text_from_another_owner_cannot_supply_semantic_context() -> None:
    raw = "{進化}{コスト９９}:これは進化する。"
    field = normalize_source(raw, source(raw))
    other = source(raw).model_copy(
        update={
            "owner": field.source.owner.model_copy(
                update={"revision_id": "revision:other"}
            )
        }
    )
    with pytest.raises(ValueError, match="another exact owner"):
        classifier().recognize(
            raw,
            field.source,
            field.parts[0],
            context=replace(context(raw), source=other),
        )


@pytest.mark.parametrize(
    ("raw", "resolved"),
    [
        ("【仮 keyword】【別 keyword】", True),
        ("【仮 keyword】を持つ。", False),
        ("【仮 keyword】を持たない。", False),
        ("条件なら、【仮 keyword】", False),
        ("【仮 keyword】【未採用】", False),
    ],
)
def test_standalone_keyword_sequence_does_not_include_grant_negation_or_condition(
    raw: str, resolved: bool
) -> None:
    engine = classifier(
        (
            Term("term:keyword.synthetic", "keyword", "仮 keyword"),
            Term("term:keyword.other", "keyword", "別 keyword"),
        )
    )
    field = normalize_source(raw, source(raw))
    found = engine.recognize(raw, field.source, field.parts[0])
    assert (found.semantics is not None) is resolved
    if resolved:
        assert found.semantics is not None
        assert found.semantics.key == "card_keywords.v1"
        assert found.semantics.projection.projection_kind == "card_field"


@pytest.mark.parametrize(
    ("anchor", "resolved"),
    [("【疾走】", True), ("【疾走】を持つ。", False), ("【突進】", False)],
)
def test_pure_reminder_requires_registered_complete_grammar_and_matching_anchor(
    anchor: str, resolved: bool
) -> None:
    reminder = "（プレイしたターンから攻撃できる。）"
    raw = anchor + reminder
    field = normalize_source(raw, source(raw), reminders=frozenset({reminder}))
    engine = classifier(
        (
            Term("term:keyword.storm", "keyword", "疾走"),
            Term("term:keyword.rush", "keyword", "突進"),
        )
    )
    part = next(p for p in field.parts if p.source_span.role == "reminder")
    found = engine.recognize(raw, field.source, part, field=field)
    assert (found.semantics is not None) is resolved
    if resolved:
        assert found.semantics is not None
        assert found.semantics.key == "pure_reminder.v1"
        assert found.semantics.projection.projection_kind == "none"


@pytest.mark.parametrize("mutation", ["key", "role", "projection", "scope"])
def test_n0_cannot_publish_an_unregistered_or_mismatched_semantic_descriptor(
    mutation: str,
) -> None:
    raw = "{進化}{コスト９９}:これは進化する。"
    field = normalize_source(raw, source(raw))
    frame, _ = (
        classifier()
        .recognize(raw, field.source, field.parts[0], context=context(raw))
        .bind(field.source, field.parts[0])
    )
    data = frame.model_dump(mode="json")
    if mutation == "key":
        data["semantic_variant"]["key"] = "unregistered.v1"
        data["projection"]["discriminator"] = "unregistered.v1"
    elif mutation == "role":
        data["role"] = "reminder"
    elif mutation == "projection":
        data["projection"]["projection_kind"] = "card_field"
    else:
        data["projection"]["scopes"][0]["id"] = "other_scope"
    with pytest.raises(ValueError, match="N0"):
        Frame.model_validate_json(canonical(data))


@pytest.mark.parametrize("missing", [None, "name", "trait", "class", "kind"])
def test_token_header_requires_every_declared_field_in_its_exact_catalog(
    missing: str | None,
) -> None:
    raw = (
        "『仮トークン😀』{仮クラス}仮種族・フォロワー{コスト２}{攻撃力}３/{体力}４仮。"
    )
    field = normalize_source(raw, source(raw, "section"))
    terms = {
        "name": Term("term:name.synthetic", "card_name", "仮トークン😀"),
        "trait": Term("term:trait.synthetic", "trait", "仮種族"),
    }
    engine = classifier(tuple(t for k, t in terms.items() if k != missing))
    vocabulary = {
        "class": Binding(region="jp", kind="class", raw="仮クラス", code="synthetic"),
        "kind": Binding(region="jp", kind="type", raw="フォロワー", code="follower"),
    }
    bindings = tuple(b for k, b in vocabulary.items() if k != missing)
    engine.references.vocabulary = Vocabulary(
        bindings=bindings,
        terms=tuple(
            VocabularyTerm(
                kind=b.kind, code=b.code, label=LocalizedText(lang="ja", text=b.raw)
            )
            for b in bindings
        ),
    )
    part = next(p for p in field.parts if p.source_span.role == "token_header")
    found = engine.recognize(raw, field.source, part, field=field)
    if missing is not None:
        assert found.issues
        assert found.semantics is None
        with pytest.raises(ValueError, match="Unresolved source leaves"):
            found.bind(field.source, part)
        return
    assert not found.issues
    frame, binding = found.bind(field.source, part)
    engine.verify(raw, field, part, frame, binding)
    assert frame.semantic_variant.key == "token_header.v1"
    assert frame.projection.projection_kind == "card_field"
    assert [s.role for s in frame.leaf_schema.slots] == [
        "declared_name",
        "declared_class",
        "declared_trait",
        "declared_kind",
        "cost_value",
        "stat_value",
        "stat_value",
    ]
    assert all(o.source_unit is None for o in binding.occurrences)


@pytest.mark.parametrize("keyword", ["疾走", "突進"])
@pytest.mark.parametrize("extra", ["", "仮の追加条件。"])
def test_reminder_needs_the_entire_registered_sentence(
    keyword: str, extra: str
) -> None:
    action = "攻撃できる" if keyword == "疾走" else "フォロワーに攻撃できる"
    reminder = f"（これはプレイしたターンから{action}。{extra}）"
    raw = f"【{keyword}】" + reminder
    field = normalize_source(raw, source(raw), reminders=frozenset({reminder}))
    engine = classifier((Term("term:keyword.synthetic", "keyword", keyword),))
    part = next(p for p in field.parts if p.source_span.role == "reminder")
    found = engine.recognize(raw, field.source, part, field=field)
    assert (found.semantics is not None) is (not extra)
