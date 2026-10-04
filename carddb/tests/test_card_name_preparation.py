"""Synthetic rejection cases for delegated name preparation, without raw card text."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.translations import card_names
from sve_carddb.translations.card_names import Candidate, mapping_hash, prepare
from sve_carddb.translations.loader import Snapshot, validate_snapshot
from sve_carddb.translations.models import ChoiceRecord, Delegation, TermRecord

from .translation_fixtures import INSTANT, reference

if TYPE_CHECKING:
    from sve_carddb.translations.models import Shard

EMPTY = Snapshot(b"", ())


def candidate(slug: str = "test_star", text: str = "合成測試星") -> Candidate:
    ref = reference(provider="jp", locator="/faces/0/name")
    ref["text_hash"] = digest(slug.encode())
    return Candidate.model_validate_json(
        canonical(
            {
                "concept_key": "name." + slug,
                "source_ref": ref,
                "text": text,
                "origin": "machine",
                "source_claim": None,
            }
        )
    )


def receipt(rows: tuple[Candidate, ...]) -> Delegation:
    parts: list[list[JsonValue]] = []
    for row in rows:
        parts.extend(
            [
                ["glossary_term", "term:" + row.concept_key],
                ["glossary_choice", "term:" + row.concept_key, "zh-Hant", 1],
            ]
        )
    keys = sorted(canonical(value).decode() for value in parts)
    return Delegation(
        authorized_by="Synthetic maintainer",
        authorization_basis="Synthetic scoped name delegation, event 1.",
        authorization_date="2026-10-02",
        scope=tuple(keys),
        decided_by="Synthetic delegated coordinator",
        decided_at=INSTANT,
        decided_precision="day",
        decision_basis="Synthetic approved key/value map " + mapping_hash(rows),
    )


def build(
    rows: tuple[Candidate, ...],
    existing: Snapshot = EMPTY,
    approval: Delegation | None = None,
) -> tuple[Shard, ...]:
    return prepare(
        rows,
        existing,
        receipt(rows) if approval is None else approval,
        authored_by="Synthetic writer",
        authored_at=INSTANT,
    )


def snapshot(shards: tuple[Shard, ...]) -> Snapshot:
    counts = {"concepts": 0, "choices": 0}
    entries = []
    for shard in shards:
        filing = shard.records[0].filing_key
        counts[filing] += 1
        content = canonical(shard.model_dump(mode="json"))
        path = f"translations/glossary/{filing}/{counts[filing]:03}.yaml"
        entries.append((path, content, content))
    return Snapshot(b"", tuple(entries))


def test_same_translation_does_not_merge_distinct_names() -> None:
    rows = (candidate("test_moon"), candidate())
    shards = build(rows)
    result = snapshot(shards)
    validate_snapshot(result)
    terms = [r for r, _ in result.records() if isinstance(r, TermRecord)]
    choices = [r for r, _ in result.records() if isinstance(r, ChoiceRecord)]
    assert {r.data.id for r in terms} == {
        "term:name.test_star",
        "term:name.test_moon",
    }
    assert {r.data.term_id for r in choices} == {r.data.id for r in terms}
    assert all(r.data.authored_source_ja is None for r in terms)
    assert all(r.data.source_ref is not None for r in terms)
    assert all(r.data.category == "card_name" for r in terms)
    assert all(r.data.origin == "machine" for r in choices)
    assert all(r.data.adoption_review.mode == "delegated_glossary" for r in choices)
    assert all(
        d.category in {"glossary_term", "glossary_choice"}
        for s in shards
        for d in s.decisions
    )
    assert result.review_counts() == {
        "human_sampled_rows": 0,
        "delegated_glossary_rows": 4,
    }


def test_receipts_cover_separate_small_shards_and_output_is_deterministic() -> None:
    rows = tuple(candidate(f"test_star_{i}") for i in range(49))
    shards = build(rows)
    assert shards == build(tuple(reversed(rows)))
    assert len(shards) == 6
    assert [len(s.records) for s in shards] == [24, 24, 24, 24, 1, 1]
    result = snapshot(shards)
    validate_snapshot(result)
    for shard in shards:
        for record in shard.records:
            assert isinstance(record, (TermRecord, ChoiceRecord))
            delegation = record.data.adoption_review.delegation
            assert delegation is not None
            assert delegation.scope == shard.decisions[0].sample_ids


@pytest.mark.parametrize(
    "slug", ["", "Test", "test__star", "test_", "test-star", "7star", "x" * 92]
)
def test_invalid_key(slug: str) -> None:
    with pytest.raises(ValidationError) as caught:
        candidate(slug)
    errors = caught.value.errors()
    assert len(errors) == 1
    assert errors[0]["loc"] == ("concept_key",)
    assert errors[0]["type"] == (
        "string_too_long" if slug == "x" * 92 else "string_pattern_mismatch"
    )


def change(row: Candidate, **fields: JsonValue) -> Candidate:
    return Candidate.model_validate_json(
        canonical({**row.model_dump(mode="json"), **fields})
    )


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        ("text", " \t", "Card-name translation must be nonblank"),
        ("origin", "project", "Borrowed card-name wording requires a source claim"),
    ],
)
def test_value_and_attribution_guards(field: str, value: str, message: str) -> None:
    row = change(candidate(), **{field: value})
    with pytest.raises(ValueError, match="^" + message + "$"):
        build((row,))


@pytest.mark.parametrize("origin", ["official_sv1", "official_svwb", "community"])
def test_no_automatic_origin_promotion(origin: str) -> None:
    with pytest.raises(ValidationError) as caught:
        change(candidate(), origin=origin)
    errors = caught.value.errors()
    assert len(errors) == 1
    assert errors[0]["loc"] == ("origin",)
    assert errors[0]["type"] == "literal_error"


def test_borrowed_full_wording_retains_unverified_claim() -> None:
    claim: dict[str, JsonValue] = {
        "source_work": "Synthetic work",
        "source_urls": ["https://example.test/names"],
        "claimed_source": "Synthetic community name list",
        "note": "Wording borrowed; no frozen official same-concept evidence.",
    }
    rows = (change(candidate(), origin="project", source_claim=claim),)
    choices = [r for s in build(rows) for r in s.records if isinstance(r, ChoiceRecord)]
    assert len(choices) == 1
    assert choices[0].data.source_claim is not None
    assert choices[0].data.source_claim.model_dump(mode="json") == claim
    assert choices[0].data.origin == "project"
    assert choices[0].data.concept_evidence == ()


@pytest.mark.parametrize(
    ("parser", "locator"),
    [
        ("translation-en-v1", "/faces/0/name"),
        ("translation-jp-v1", "/faces/0/text"),
        ("translation-jp-v1", "/faces/00/name"),
    ],
)
def test_name_source_guard(parser: str, locator: str) -> None:
    row = candidate()
    data = row.source_ref.model_dump(mode="json")
    data.update(parser=parser, locator=locator)
    row = change(row, source_ref=data)
    with pytest.raises(
        ValueError, match=r"^Card-name concept requires a frozen Japanese name field$"
    ):
        build((row,))


def test_duplicate_key() -> None:
    row = candidate()
    with pytest.raises(ValueError, match=r"^Card-name concept keys must be unique$"):
        build((row, row), approval=receipt((row,)))


def test_duplicate_exact_source_under_two_keys() -> None:
    first = candidate()
    second = candidate("test_moon")
    second = change(second, source_ref=first.source_ref.model_dump(mode="json"))
    with pytest.raises(
        ValueError, match=r"^Exact card names require one proposed concept$"
    ):
        build((first, second))


def test_allocated_key() -> None:
    row = candidate()
    existing = snapshot(build((row,)))
    with pytest.raises(
        ValueError, match=r"^Card-name concept key is already allocated$"
    ):
        build((row,), existing)


def test_existing_exact_name_does_not_allocate_alias_key() -> None:
    row = candidate()
    other = change(row, concept_key="name.other_star")
    existing = snapshot(build((row,)))
    with pytest.raises(
        ValueError, match=r"^Exact card name already has an adopted concept$"
    ):
        build((other,), existing)


def test_no_candidates() -> None:
    with pytest.raises(
        ValueError, match=r"^Card-name preparation requires candidates$"
    ):
        build((), approval=receipt((candidate(),)))


def test_scope_cannot_borrow_smaller_delegation() -> None:
    row = candidate()
    other = candidate("test_moon")
    with pytest.raises(
        ValueError,
        match=r"^Card-name delegation scope must cover the exact prepared map$",
    ):
        build((row, other), approval=receipt((row,)))


def test_changed_value_requires_new_approved_map() -> None:
    row = candidate()
    changed = change(row, text="另一個合成譯名")
    with pytest.raises(
        ValueError,
        match=r"^Card-name decision must identify the approved complete map$",
    ):
        build((changed,), approval=receipt((row,)))


def test_claim_change_requires_new_approved_map() -> None:
    row = candidate()
    changed = change(
        row,
        source_claim={
            "source_work": "Synthetic work",
            "source_urls": [],
            "claimed_source": None,
            "note": "Partial borrowed component, complete wording remains machine generated.",
        },
    )
    with pytest.raises(
        ValueError,
        match=r"^Card-name decision must identify the approved complete map$",
    ):
        build((changed,), approval=receipt((row,)))


def test_invalid_delegation_scope_does_not_depend_on_record_order() -> None:
    row = candidate()
    approval = receipt((row,))
    data = approval.model_dump(mode="json")
    data["scope"] = list(reversed(approval.scope))
    with pytest.raises(ValidationError) as caught:
        Delegation.model_validate_json(canonical(data))
    errors = caught.value.errors()
    assert len(errors) == 1
    assert errors[0]["loc"] == ()
    assert errors[0]["msg"] == "Value error, Delegation scope must be sorted and unique"


def test_candidate_cannot_carry_original_name_field() -> None:
    with pytest.raises(ValidationError) as caught:
        change(candidate(), ja="Synthetic original forbidden in output")
    errors = caught.value.errors()
    assert len(errors) == 1
    assert errors[0]["loc"] == ("ja",)
    assert errors[0]["type"] == "extra_forbidden"


def test_size_limit(monkeypatch: pytest.MonkeyPatch) -> None:
    row = candidate()
    monkeypatch.setattr(card_names, "MAX_BYTES", 1)
    with pytest.raises(
        ValueError, match=r"^Prepared card-name shard exceeds the size limit$"
    ):
        build((row,))


def test_preparation_does_not_mutate_existing_snapshot() -> None:
    existing = replace(EMPTY, index=b"synthetic immutable index")
    build((candidate(),), existing)
    assert existing == replace(EMPTY, index=b"synthetic immutable index")
