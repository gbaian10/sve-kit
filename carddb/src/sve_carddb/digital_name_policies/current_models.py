"""Editable name rules contain business conditions, not an approval event graph."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves nested models at runtime

from typing import Annotated, Literal

from pydantic import Field, model_validator

from sve_carddb.digital_name_policies.models import Entry, PolicyId
from sve_carddb.products.models import Lang
from sve_carddb.registry.records import Hash, RecordData, Text
from sve_carddb.translations.current_models import Origin, Owner

CodePoint = Annotated[int, Field(ge=0, le=0x10FFFF)]


class Scope(RecordData):
    field: Literal["name"]
    owners: tuple[Literal["face_revision.name"], Literal["known_printing_face.name"]]
    region: Literal["jp"]
    source_lang: Literal["ja"]
    target_lang: Literal["zh-Hant"]


class Minimum(RecordData):
    kana_ranges: tuple[tuple[CodePoint, CodePoint], ...]
    whitespace_codepoints: tuple[CodePoint, ...]
    trim_or_normalize: Literal[False]

    @model_validator(mode="after")
    def _ranges(self) -> Minimum:
        previous = -1
        for first, last in self.kana_ranges:
            if first > last or first <= previous:
                raise ValueError(
                    "Name minimum-check ranges must be sorted and nonoverlapping"
                )
            previous = last
        if self.whitespace_codepoints != tuple(sorted(set(self.whitespace_codepoints))):
            raise ValueError("Name whitespace codepoints must be sorted and unique")
        return self


class Override(RecordData):
    owner: Owner
    source_hash: Hash
    term_id: Text


class Exclusion(RecordData):
    source_lang: Lang
    source_name_hash: Hash
    reason: Text

    @model_validator(mode="after")
    def _reason(self) -> Exclusion:
        if not self.reason.strip():
            raise ValueError("Name exclusion requires a reason")
        return self


class Content(RecordData):
    scope: Scope
    game_priority: tuple[Literal["sv1"], Literal["svwb"]]
    target_minimum_check: Minimum
    excluded_names: tuple[Exclusion, ...]
    name_overrides: tuple[Override, ...]

    @model_validator(mode="after")
    def _unique(self) -> Content:
        hashes = tuple(e.source_name_hash for e in self.excluded_names)
        if hashes != tuple(sorted(set(hashes))):
            raise ValueError("Name exclusions must be sorted and unique")
        from sve_carddb.snapshot.values import canonical  # ruff: ignore[import-outside-top-level] -- shared canonical owner keys prevent delimiter collisions

        keys = [
            canonical([o.owner.model_dump(mode="json"), o.source_hash])
            for o in self.name_overrides
        ]
        if len(keys) != len(set(keys)):
            raise ValueError("Duplicate current name override owner/source")
        return self


class Policy(RecordData):
    digital_name_policy_format: Literal[2]
    kind: Literal["digital_name_policy"]
    policy_id: PolicyId
    purpose: Literal["names"]
    content: Content
    origin: Origin
    low_confidence: bool
    note: str = ""


class CurrentEntry(RecordData):
    path: Text
    hash: Hash


class Index(RecordData):
    digital_name_policy_index_format: Literal[2]
    kind: Literal["digital_name_policy_index"]
    policies: Annotated[
        dict[PolicyId, CurrentEntry | tuple[Entry, ...]], Field(min_length=1)
    ]
