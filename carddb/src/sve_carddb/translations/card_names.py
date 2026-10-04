"""Prepare card-name shards with separate key-allocation and word-choice events."""

import re
from collections.abc import Mapping  # ruff: ignore[typing-only-standard-library-import] -- the public mapping annotation also documents immutable caller inputs
from typing import Annotated, Literal

from pydantic import Field

from sve_carddb.catalog.adoption_models import SourceRef  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves this inherited model field at runtime
from sve_carddb.registry.records import Hash, Instant, RecordData, Text
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


class IndividualApproval(RecordData):
    kind: Literal["individual"]
    reviewed_by: Literal["gbaian10"]
    reviewed_at: Instant
    basis: Text
    values: Annotated[tuple[tuple[Text, Hash], ...], Field(min_length=1)]


ChoiceEvent = Delegation | IndividualApproval


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
    candidate: Candidate, term_review: AdoptionReview, choice_review: AdoptionReview
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
            adoption_review=term_review,
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
            adoption_review=choice_review,
            adoption_no=1,
            predecessor=None,
        ),
        evidence=(),
    )
    return term, choice


def _shard(
    records: tuple[TermRecord | ChoiceRecord, ...],
    receipt: ChoiceEvent,
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
        reviewed_by=(
            receipt.decided_by
            if isinstance(receipt, Delegation)
            else receipt.reviewed_by
        ),
        reviewed_at=(
            receipt.decided_at
            if isinstance(receipt, Delegation)
            else receipt.reviewed_at
        ),
        reviewed_precision=(
            receipt.decided_precision if isinstance(receipt, Delegation) else "instant"
        ),
        note=NOTE + "；" + receipt.decision_basis
        if isinstance(receipt, Delegation)
        else "維護者逐項核可；" + receipt.basis,
        state="confirmed",
        category=ordered[0].kind,
        policy_id="delegated-card-names-v1"
        if isinstance(receipt, Delegation)
        else "human-card-names-v1",
    )
    return Shard(
        translation_authored_format=1,
        kind="translation_shard",
        default_decision_id=identifier,
        records=ordered,
        decisions=(decision,),
    )


def _review(event: ChoiceEvent, keys: tuple[str, ...]) -> AdoptionReview:
    if isinstance(event, IndividualApproval):
        return AdoptionReview(mode="human", delegation=None)
    # Per-shard scopes avoid quadratically repeating the entire event in every record.
    narrowed = Delegation.model_validate_json(
        canonical({**event.model_dump(mode="json"), "scope": list(keys)})
    )
    return AdoptionReview(mode="delegated_glossary", delegation=narrowed)


def _events(
    candidates: tuple[Candidate, ...], choices: Mapping[str, ChoiceEvent]
) -> list[tuple[ChoiceEvent, list[Candidate]]]:
    if set(choices) != {c.concept_key for c in candidates}:
        raise ValueError("Card-name choices require one covering event per candidate")
    groups: dict[bytes, tuple[ChoiceEvent, list[Candidate]]] = {}
    for candidate in candidates:
        event = choices[candidate.concept_key]
        identity = canonical(event.model_dump(mode="json"))
        if identity not in groups:
            groups[identity] = event, []
        groups[identity][1].append(candidate)
    for event, rows in groups.values():
        keys = tuple(sorted(_keys(c)[1] for c in rows))
        if isinstance(event, Delegation):
            if event.scope != keys:
                raise ValueError(
                    "Card-name choice event scope must match its assigned records"
                )
            if mapping_hash(candidates) not in event.decision_basis:
                raise ValueError(
                    "Card-name decision must identify the approved complete map"
                )
        else:
            values = tuple(sorted((_keys(c)[1], digest(c.text.encode())) for c in rows))
            if event.values != values:
                raise ValueError(
                    "Individual card-name approval must match exact assigned values"
                )
            if not event.basis.strip():
                raise ValueError(
                    "Individual card-name approval requires event evidence"
                )
    return [groups[key] for key in sorted(groups)]


def prepare(
    candidates: tuple[Candidate, ...],
    existing: Snapshot,
    receipt: Delegation,
    *,
    choices: Mapping[str, ChoiceEvent],
    authored_by: Text,
    authored_at: Instant,
) -> tuple[Shard, ...]:
    """Require caller-supplied events; this tool cannot establish approval itself."""
    _check(candidates, existing)
    scope = tuple(sorted(_keys(c)[0] for c in candidates))
    if receipt.scope != scope:
        raise ValueError("Card-name delegation scope must cover the exact prepared map")
    if mapping_hash(candidates) not in receipt.decision_basis:
        raise ValueError("Card-name decision must identify the approved complete map")
    groups = [(receipt, list(candidates), 0)] + [
        (event, rows, 1) for event, rows in _events(candidates, choices)
    ]
    result = []
    for event, rows, kind_index in groups:
        ordered = sorted(rows, key=lambda candidate: candidate.concept_key)
        for start in range(0, len(ordered), CHUNK_SIZE):
            group = ordered[start : start + CHUNK_SIZE]
            subset = tuple(sorted(_keys(c)[kind_index] for c in group))
            review = _review(event, subset)
            records = tuple(_records(c, review, review)[kind_index] for c in group)
            shard = _shard(records, event, authored_by, authored_at)
            if len(encode(shard)) >= MAX_BYTES:
                raise ValueError("Prepared card-name shard exceeds the size limit")
            result.append(shard)
    return tuple(result)
