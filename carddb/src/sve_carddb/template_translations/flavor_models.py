"""Independent flavor wire types do not alter the frozen effect recognition recipe."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves wire annotations

from dataclasses import dataclass
from typing import Annotated, Literal

from pydantic import Field, field_validator

from sve_carddb.catalog.adoption_models import Batch, SourceRef
from sve_carddb.registry.records import Hash, RecordData, Text
from sve_carddb.template_parameters.models import Range
from sve_carddb.translations.models import IdentityBasis


class FlavorSpan(RecordData):
    role: Literal["flavor"]
    segments: Annotated[tuple[Range, ...], Field(min_length=1, max_length=1)]
    anchor: None


class FlavorEntry(RecordData):
    id: Text
    level: Literal["sentence"]
    source_ref: SourceRef
    line_ordinal: Literal[0]
    role: Literal["flavor"]
    normalizer_id: Literal["flavor-exact-v1"]
    normalized_hash: Hash
    legacy_fingerprint: None

    @field_validator("line_ordinal", mode="before")
    @classmethod
    def integer_zero(cls, value: object) -> object:
        """Literal equality must not turn false/float into a source coordinate."""
        if type(value) is not int:
            raise ValueError("Flavor line ordinal must be integer zero")
        return value


@dataclass(frozen=True)
class FlavorCandidate:
    source_span: FlavorSpan
    legacy_id: None = None


@dataclass(frozen=True)
class FlavorOwner:
    printing_id: str
    face_id: str
    flavor_unit_id: str

    def payload(self) -> dict[str, str]:
        """Physical owners remain distinct even when their paragraphs share bytes."""
        return {
            "kind": "printing_face",
            "printing_id": self.printing_id,
            "face_id": self.face_id,
        }


class FlavorInputs(RecordData):
    source_batch: Batch
    identity_basis: IdentityBasis | None
    identity_batches: tuple[Batch, ...]
