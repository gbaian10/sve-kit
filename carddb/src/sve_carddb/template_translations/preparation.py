"""Explicit target coordinates prepare candidates; confidence and bare verdicts never adopt them."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves constrained nested types

from collections import Counter
from itertools import pairwise
from typing import Annotated, Literal, Self

from pydantic import Field, model_validator

from sve_carddb.registry.records import Hash, RecordData, Text
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameters.models import Range, Schema
from sve_carddb.template_translations.text import escape, parse


class Draft(RecordData):
    template: Annotated[str, Field(pattern=r"^T[0-9a-f]{10}\Z")]
    normalized: Text
    zh: Text
    confidence: Literal["high", "medium", "low"]
    note: str

    @model_validator(mode="after")
    def _fingerprint(self) -> Self:
        if self.template != "T" + digest(self.normalized.encode())[7:17]:
            raise ValueError(
                "Translation draft must reproduce its legacy normalized fingerprint"
            )
        return self


class TargetSlot(RecordData):
    name: Text
    span: Range
    raw_hash: Hash


class Alignment(RecordData):
    draft_text_hash: Hash
    schema_hash: Hash
    slots: tuple[TargetSlot, ...]

    @model_validator(mode="after")
    def _order(self) -> Self:
        if any(a.span.end > b.span.start for a, b in pairwise(self.slots)):
            raise ValueError(
                "Translation target slot spans must be sorted and disjoint"
            )
        return self


class Prepared(RecordData):
    template_id: Text
    legacy_id: Text
    lang: Literal["zh-Hant"] = "zh-Hant"
    text: Text
    text_hash: Hash
    origin: Literal["machine"] = "machine"
    confidence: Literal["high", "medium", "low"]
    state: Literal["candidate_only"] = "candidate_only"


def convert(
    draft: Draft, schema: Schema, alignment: Alignment, *, template_id: str
) -> Prepared:
    """Use verified explicit target spans, never pair indistinguishable N's by ordinal."""
    if alignment.draft_text_hash != digest(draft.zh.encode()):
        raise ValueError("Translation alignment must pin the exact draft text")
    if alignment.schema_hash != digest(canonical(schema.model_dump(mode="json"))):
        raise ValueError("Translation alignment must pin the exact parameter schema")
    declared = {slot.name: len(slot.occurrences) for slot in schema.slots}
    if Counter(slot.name for slot in alignment.slots) != Counter(declared):
        raise ValueError(
            "Translation alignment must cover each schema occurrence exactly once"
        )
    position = 0
    output: list[str] = []
    for slot in alignment.slots:
        if slot.span.end > len(draft.zh):
            raise ValueError(
                "Translation target span must be inside the exact draft text"
            )
        if digest(draft.zh[slot.span.start : slot.span.end].encode()) != slot.raw_hash:
            raise ValueError("Translation target span must match its exact raw hash")
        output.extend(
            (escape(draft.zh[position : slot.span.start]), "{{" + slot.name + "}}")
        )
        position = slot.span.end
    output.append(escape(draft.zh[position:]))
    text = "".join(output)
    parse(text, schema)
    return Prepared(
        template_id=template_id,
        legacy_id=draft.template,
        text=text,
        text_hash=digest(text.encode()),
        confidence=draft.confidence,
    )
