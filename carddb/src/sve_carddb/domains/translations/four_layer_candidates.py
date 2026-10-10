"""Unactivated translation drafts are distinct from executable targets."""

from typing import Annotated, Literal, Self

from pydantic import Field, computed_field, field_validator, model_validator

from sve_carddb.contracts.four_layer import Role
from sve_carddb.core.json import canonical
from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.domains.products.models import Lang


class Candidate(RecordData):
    source_kind: Literal["effect"]
    candidate_id: Text
    lang: Lang
    text: Text
    normalized_hash: Hash
    role: Role
    reasons: Annotated[
        tuple[Annotated[str, Field(pattern=r"^[a-z][a-z0-9_]*\Z")], ...],
        Field(min_length=1),
    ]

    @field_validator("text")
    @classmethod
    def _utf8(cls, value: str) -> str:
        value.encode("utf-8")
        return value

    @model_validator(mode="after")
    def _ordered(self) -> Self:
        if self.reasons != tuple(sorted(set(self.reasons))):
            raise ValueError("Candidate reasons must be sorted and unique")
        return self


class CandidateRecord(RecordData):
    kind: Literal["template_translation_candidate"]
    data: Candidate
    origin: Literal["project", "machine"] = "project"
    low_confidence: bool = False
    note: str = ""

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes this property; mypy cannot compose property decorators.
    @property
    def record_key(self) -> str:
        """Derive identity independently of mutable values and authored metadata."""
        return canonical(
            [self.kind, self.data.source_kind, self.data.candidate_id, self.data.lang]
        ).decode()
