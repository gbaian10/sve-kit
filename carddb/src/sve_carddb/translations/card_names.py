"""Prepare delegated card-name shards from an explicitly approved key/value map."""

import re
from typing import Annotated, Literal

from pydantic import Field

from sve_carddb.catalog.adoption_models import SourceRef  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves this inherited model field at runtime
from sve_carddb.registry.records import Instant, RecordData, Text
from sve_carddb.registry.storage import MAX_BYTES, encode
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.translations.loader import Snapshot, record_hash
from sve_carddb.translations.models import (
    AdoptionReview,
    AuthoredValue,
    ChoiceData,
    ChoiceRecord,
    Decision,
    Delegation,
    Shard,
    SourceClaim,
    TermData,
    TermRecord,
)

NOTE = "維護者委託；協調者決定；不是維護者親自核可"
CHUNK_SIZE = 24
NameKey = Annotated[
    str,
    Field(pattern=r"^name\.[a-z][a-z0-9]*(?:_[a-z0-9]+)*\Z", max_length=96),
]


class Candidate(RecordData):
    concept_key: NameKey
    source_ref: SourceRef
    text: Text
    origin: Literal["project", "machine"]
    source_claim: SourceClaim | None


def mapping_hash(candidates: tuple[Candidate, ...]) -> str:
    """Bind approval to the complete map, including exact value and attribution."""
    ordered = sorted(candidates, key=lambda candidate: candidate.concept_key)
    return digest(canonical([c.model_dump(mode="json") for c in ordered]))


def _keys(candidate: Candidate) -> tuple[str, str]:
    identifier = "term:" + candidate.concept_key
    return (
        canonical(["glossary_term", identifier]).decode(),
        canonical(["glossary_choice", identifier, "zh-Hant", 1]).decode(),
    )


def _check(candidates: tuple[Candidate, ...], existing: Snapshot) -> None:
    if not candidates:
        raise ValueError("Card-name preparation requires candidates")
    names = [c.concept_key for c in candidates]
    if len(set(names)) != len(names):
        raise ValueError("Card-name concept keys must be unique")
    hashes = [c.source_ref.text_hash for c in candidates]
    if len(set(hashes)) != len(hashes):
        raise ValueError("Exact card names require one proposed concept")
    old_keys = {
        r.data.concept_key for r, _ in existing.records() if isinstance(r, TermRecord)
    }
    if old_keys & set(names):
        raise ValueError("Card-name concept key is already allocated")
    old_hashes = {
        r.data.source_ref.text_hash
        for r, _ in existing.records()
        if isinstance(r, TermRecord)
        and r.data.category == "card_name"
        and r.data.source_ref is not None
        and r.data.source_span is None
    }
    if old_hashes & set(hashes):
        raise ValueError("Exact card name already has an adopted concept")
    for candidate in candidates:
        ref = candidate.source_ref
        if ref.parser != "translation-jp-v1" or not re.fullmatch(
            r"/faces/(?:0|[1-9][0-9]*)/name", ref.locator
        ):
            raise ValueError("Card-name concept requires a frozen Japanese name field")
        if not candidate.text.strip():
            raise ValueError("Card-name translation must be nonblank")
        if candidate.origin == "project" and candidate.source_claim is None:
            raise ValueError("Borrowed card-name wording requires a source claim")


def _records(
    candidate: Candidate, review: AdoptionReview
) -> tuple[TermRecord, ChoiceRecord]:
    term_key, choice_key = _keys(candidate)
    identifier = "term:" + candidate.concept_key
    term = TermRecord(
        record_key=term_key,
        kind="glossary_term",
        filing_key="concepts",
        data=TermData(
            id=identifier,
            category="card_name",
            concept_key=candidate.concept_key,
            source_ref=candidate.source_ref,
            source_span=None,
            authored_source_ja=None,
            missing_source_reason=None,
            adoption_review=review,
        ),
        evidence=(),
    )
    choice = ChoiceRecord(
        record_key=choice_key,
        kind="glossary_choice",
        filing_key="choices",
        data=ChoiceData(
            term_id=identifier,
            lang="zh-Hant",
            value=AuthoredValue(kind="authored", text=candidate.text),
            origin=candidate.origin,
            concept_evidence=(),
            source_claim=candidate.source_claim,
            adoption_review=review,
            adoption_no=1,
            predecessor=None,
        ),
        evidence=(),
    )
    return term, choice


def _shard(
    records: tuple[TermRecord | ChoiceRecord, ...],
    receipt: Delegation,
    authored_by: str,
    authored_at: str,
) -> Shard:
    ordered = tuple(sorted(records, key=lambda record: record.record_key))
    members = tuple((r.record_key, record_hash(r)) for r in ordered)
    checksum = digest(canonical([[k, h] for k, h in members]))
    identifier = "d:" + checksum.removeprefix("sha256:")
    decision = Decision(
        id=identifier,
        scope="batch",
        membership_hash=checksum,
        members=members,
        sample_ids=tuple(r.record_key for r in ordered),
        authored_by=authored_by,
        authored_at=authored_at,
        reviewed_by=receipt.decided_by,
        reviewed_at=receipt.decided_at,
        reviewed_precision=receipt.decided_precision,
        note=NOTE,
        state="confirmed",
        category=ordered[0].kind,
        policy_id="delegated-card-names-v1",
    )
    return Shard(
        translation_authored_format=1,
        kind="translation_shard",
        default_decision_id=identifier,
        records=ordered,
        decisions=(decision,),
    )


def prepare(
    candidates: tuple[Candidate, ...],
    existing: Snapshot,
    receipt: Delegation,
    *,
    authored_by: Text,
    authored_at: Instant,
) -> tuple[Shard, ...]:
    """Return append-only envelopes; source replay and publication stay with the loader."""
    _check(candidates, existing)
    scope = tuple(sorted(k for c in candidates for k in _keys(c)))
    if receipt.scope != scope:
        raise ValueError("Card-name delegation scope must cover the exact prepared map")
    if mapping_hash(candidates) not in receipt.decision_basis:
        raise ValueError("Card-name decision must identify the approved complete map")
    result = []
    ordered = sorted(candidates, key=lambda candidate: candidate.concept_key)
    # Per-shard scopes avoid quadratically repeating the entire batch in every record.
    for start in range(0, len(ordered), CHUNK_SIZE):
        group = ordered[start : start + CHUNK_SIZE]
        for kind_index in (0, 1):
            subset = tuple(sorted(_keys(c)[kind_index] for c in group))
            narrowed = Delegation.model_validate_json(
                canonical({**receipt.model_dump(mode="json"), "scope": list(subset)})
            )
            review = AdoptionReview(mode="delegated_glossary", delegation=narrowed)
            pairs = [_records(c, review) for c in group]
            records = (
                tuple(pair[0] for pair in pairs)
                if kind_index == 0
                else tuple(pair[1] for pair in pairs)
            )
            shard = _shard(records, narrowed, authored_by, authored_at)
            if len(encode(shard)) >= MAX_BYTES:
                raise ValueError("Prepared card-name shard exceeds the size limit")
            result.append(shard)
    return tuple(result)
