"""Closed portable policy envelopes, separate from human digital-link adoptions."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves constrained and nested annotations at runtime

from typing import Annotated, Literal

from pydantic import Field, JsonValue, field_validator

from sve_carddb.build_inputs import Revision
from sve_carddb.catalog.adoption_models import Batch, Normalizer
from sve_carddb.registry.records import CardId, Hash, RecordData, Text

PolicyId = Annotated[str, Field(pattern=r"^[A-Za-z0-9_-]+\Z")]
Positive = Annotated[int, Field(ge=1)]
Purpose = Literal["links"]


class Envelope(RecordData):
    policy_id: PolicyId
    version: Positive

    @field_validator("version", mode="before")
    @classmethod
    def version_integer(cls, value: object) -> object:
        """Reject boolean versions before integer validation."""
        if type(value) is not int:
            raise ValueError("Policy version must be an integer")
        return value


class Policy(Envelope):
    digital_name_policy_format: Literal[1]
    kind: Literal["digital_name_policy"]
    purpose: Purpose
    approved_document_hash: Hash
    projection_recipe: Literal["approved-digital-name-document-v1"]
    content: dict[str, JsonValue]


class NameExclusion(RecordData):
    source_lang: Literal["ja"]
    source_name_hash: Hash
    reason: Text


class LinkNameExclusion(NameExclusion):
    kind: Literal["name"]


class CardTargetExclusion(RecordData):
    kind: Literal["card_target"]
    card_id: CardId
    game: Literal["sv1", "svwb"]
    official_id: Text
    reason: Text


class Exclusions(Envelope):
    digital_name_exclusion_format: Literal[1]
    kind: Literal["digital_name_initial_exclusions"]
    purpose: Purpose
    approved_list_hash: Hash
    entries: tuple[LinkNameExclusion | CardTargetExclusion, ...]


class Entry(RecordData):
    version: Positive
    path: Text
    hash: Hash
    exclusions_path: Text
    exclusions_hash: Hash
    predecessor: Hash | None

    @field_validator("version", mode="before")
    @classmethod
    def version_integer(cls, value: object) -> object:
        """Reject boolean versions before integer validation."""
        return Envelope.version_integer(value)


class Index(RecordData):
    digital_name_policy_index_format: Literal[1]
    kind: Literal["digital_name_policy_index"]
    policies: Annotated[dict[PolicyId, tuple[Entry, ...]], Field(min_length=1)]


class RegistryPin(RecordData):
    authored_revision: Revision
    index_path: Literal["authored/ids/index.yaml"]
    index_hash: Hash


class CatalogueConfiguration(RecordData):
    catalog_registry: RegistryPin
    digital_link_sources: tuple[Batch, ...]
    translation_recipes: dict[str, Normalizer]


class CataloguePins(RecordData):
    approved_coverage_claim: Literal[False]
    candidate_counts: dict[str, JsonValue]
    count_replay_main_revision: Revision
    extra_unicode_scan_hash: Hash
    parser_and_registry_configuration: CatalogueConfiguration
    private_name_list_hash: Hash
    r2_inventory_evidence_hash: Hash
    source_batches: tuple[Batch, ...]


class LinkRegistryPins(RecordData):
    revision: Revision
    index_hash: Hash
    card_projection_evidence_hash: Hash
    source_replay_revision: Revision
