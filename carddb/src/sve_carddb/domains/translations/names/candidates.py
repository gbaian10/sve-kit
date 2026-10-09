"""Prepare editable card-name values without manufacturing approval evidence."""


# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves inherited candidate models at runtime

import re
from typing import Annotated, Literal

from pydantic import Field

from sve_carddb.core.json import canonical
from sve_carddb.core.models import RecordData, Text
from sve_carddb.core.yaml import MAX_BYTES
from sve_carddb.domains.catalog.adoption_models import SourceRef
from sve_carddb.domains.translations.glossary.records import (
    ChoiceData,
    ChoiceRecord,
    Shard,
    TermData,
    TermRecord,
)
from sve_carddb.domains.translations.inputs import Snapshot
from sve_carddb.domains.translations.models import AuthoredValue, SourceClaim

NameKey = Annotated[
    str, Field(pattern=r"^name\.[a-z][a-z0-9]*(?:_[a-z0-9]+)*\Z", max_length=96)
]


class Candidate(RecordData):
    concept_key: NameKey
    source_ref: SourceRef
    text: Text
    origin: Literal["project", "machine"]
    low_confidence: bool
    source_claim: SourceClaim | None = None
    note: str = ""


def prepare(  # ruff: ignore[complex-structure] -- independent permanent key, exact source and source attribution checks
    candidates: tuple[Candidate, ...],
    existing: Snapshot,
) -> tuple[Shard, ...]:
    """Allocate caller-selected stable keys, preserving source attribution and quality."""
    if not candidates:
        raise ValueError("Card-name preparation requires candidates")
    proposed = [c.concept_key for c in candidates]
    if len(set(proposed)) != len(proposed):
        raise ValueError("Card-name concept keys must be unique")
    old = [r for r in existing.current_records() if isinstance(r, TermRecord)]
    if set(proposed) & {r.data.concept_key for r in old}:
        raise ValueError("Card-name concept key is already allocated")
    hashes = [c.source_ref.text_hash for c in candidates]
    if len(set(hashes)) != len(hashes):
        raise ValueError("Exact card names require one proposed concept")
    if set(hashes) & {
        r.data.source_ref.text_hash
        for r in old
        if r.data.category == "card_name"
        and r.data.source_ref is not None
        and r.data.source_span is None
    }:
        raise ValueError("Exact card name already has a current concept")
    records: list[TermRecord | ChoiceRecord] = []
    for c in sorted(candidates, key=lambda row: row.concept_key):
        if c.source_ref.parser != "translation-jp-v1" or not re.fullmatch(
            r"/faces/(?:0|[1-9][0-9]*)/name", c.source_ref.locator
        ):
            raise ValueError("Card-name concept requires a frozen Japanese name field")
        if not c.text.strip():
            raise ValueError("Card-name translation must be nonblank")
        term = TermRecord(
            kind="glossary_term",
            data=TermData(
                id="term:" + c.concept_key,
                category="card_name",
                concept_key=c.concept_key,
                source_ref=c.source_ref,
                source_span=None,
                authored_source_ja=None,
                missing_source_reason=None,
            ),
            origin="project",
            low_confidence=False,
            note=c.note,
        )
        choice = ChoiceRecord(
            kind="glossary_choice",
            data=ChoiceData(
                term_id=term.data.id,
                lang="zh-Hant",
                value=AuthoredValue(kind="authored", text=c.text),
                concept_evidence=(),
                source_claim=c.source_claim,
            ),
            origin=c.origin,
            low_confidence=c.low_confidence,
            note=c.note,
        )
        records.extend((term, choice))
    # Split record kinds for the existing filing contract; no batch decision is required.
    output = []
    for kind in ("glossary_term", "glossary_choice"):
        selected = sorted(
            (r for r in records if r.kind == kind), key=lambda r: r.record_key
        )
        for start in range(0, len(selected), 24):
            shard = Shard(
                format=2,
                kind="translation_shard",
                records=tuple(selected[start : start + 24]),
            )
            if len(canonical(shard.model_dump(mode="json"))) >= MAX_BYTES:
                raise ValueError("Card-name shard exceeds size limit")
            output.append(shard)
    return tuple(output)
