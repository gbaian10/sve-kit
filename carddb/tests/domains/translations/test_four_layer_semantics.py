"""Whole construction and owner context decide resolution independently of targets."""

from dataclasses import replace

import pytest

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
        "face:evolved",
    )


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
    ("phase", "kind", "paired"),
    [
        ("evolved", "follower", "face:evolved"),
        ("normal", "spell", "face:evolved"),
        ("normal", "follower", None),
    ],
)
def test_entry_with_missing_or_wrong_face_context_stays_pending(
    phase: str,
    kind: str,
    paired: str | None,
) -> None:
    raw = "{進化}{コスト９９}:これは進化する。"
    field = normalize_source(raw, source(raw))
    facts = replace(context(raw), phase=phase, card_kind=kind, evolved_face_id=paired)
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
