"""Portable source identity and complete physical-page parse results."""

from typing import Annotated, Final, Literal, Self

from pydantic import Field, model_validator

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import Hash, Instant, RecordData, Text, UInt
from sve_carddb.core.provenance import Revision, Version
from sve_carddb.core.regions import Region
from sve_carddb.domains.card_extras.models import RelatedLink
from sve_carddb.domains.products.identity_models import Match
from sve_carddb.domains.text_observations.presence import EffectPresence
from sve_carddb.ingest.archive.source_archive import Descriptor
from sve_carddb.parse.pages import extract_en, extract_jp, official_en, official_jp

RECIPE: Final = "sve-parsed-card-v1"
MAX_BYTES = 1024 * 1024
MAX_FACES = 2
RANGE_WIDTH: Final = 100
BATCH_PART_SIZE: Final = 1000
Family = Annotated[str, Field(pattern=r"^[A-Za-z0-9_-]+$", max_length=64)]


class Evidence(RecordData):
    descriptor: Descriptor
    descriptor_sha256: Hash
    fetched_at: Instant
    etag: str | None
    last_modified: str | None

    @model_validator(mode="after")
    def check_descriptor(self) -> Self:
        """Reject source identities disconnected from the original archive evidence."""
        descriptor = self.descriptor
        if descriptor.provider not in {"jp", "en"} or descriptor.kind != "card":
            raise ValueError("Parsed cards require SVE card-page sources")
        if (
            digest(canonical(descriptor.model_dump(mode="json")))
            != self.descriptor_sha256
        ):
            raise ValueError("Parsed source descriptor hash mismatch")
        source_key = digest(
            canonical(
                {"provider": descriptor.provider, "kind": "card", "url": descriptor.url}
            )
        )
        version = (
            "src:v1:"
            + digest(
                canonical(
                    {"source_key": source_key, "raw_sha256": descriptor.raw_sha256}
                )
            )[7:]
        )
        if descriptor.source_key != source_key or descriptor.id != version:
            raise ValueError("Parsed source identity mismatch")
        return self


class QAIdentity(RecordData):
    stable_source_key: Text
    official_number: Text | None
    locator: Text
    published_on: str | None
    date_raw: str | None


class ProductEvidence(RecordData):
    ordinal: UInt
    name: str
    date_raw: str | None
    href_raw: tuple[str, ...]
    resolved_urls: tuple[str, ...]
    matches: tuple[Match, ...]
    diagnostics: tuple[str, ...]


class LayoutEvidence(RecordData):
    presence: tuple[EffectPresence, ...]
    whitespace: tuple[str | None, ...]
    products: tuple[ProductEvidence, ...]
    qa: tuple[QAIdentity, ...]
    related: tuple[RelatedLink, ...]
    errata_urls: tuple[str, ...]


class JPDocument(RecordData):
    region: Literal["jp"]
    record: extract_jp.CardRecord


class ENDocument(RecordData):
    region: Literal["en"]
    record: extract_en.CardRecord


class ParsedObservation(RecordData):
    recipe: Literal["sve-parsed-card-v1"] = RECIPE
    producer_revision: Revision
    evidence: Evidence
    document: Annotated[JPDocument | ENDocument, Field(discriminator="region")]
    layout: LayoutEvidence
    content_hash: Hash

    def calculated_hash(self) -> str:
        """Bind the parse result independently of later batch selections."""
        return digest(canonical(self.model_dump(mode="json", exclude={"content_hash"})))

    @model_validator(mode="after")
    def check_content(self) -> Self:
        """Bind card, region, ordered faces and evidence to one source version."""
        record, region = self.document.record, self.document.region
        descriptor = self.evidence.descriptor
        adapter = official_jp if region == "jp" else official_en
        if descriptor.provider != region or descriptor.url != adapter.card_url(
            record.number
        ):
            raise ValueError("Parsed source region/card identity mismatch")
        if not 1 <= len(record.faces) <= MAX_FACES:
            raise ValueError("Parsed card requires one or two ordered faces")
        if len(self.layout.presence) != len(record.faces) or len(
            self.layout.whitespace
        ) != len(record.faces):
            raise ValueError("Parsed face evidence is incomplete")
        for ordinal, presence in enumerate(self.layout.presence):
            if (presence.result.source_version_id, presence.result.source_index) != (
                descriptor.id,
                ordinal,
            ):
                raise ValueError("Parsed face evidence belongs to another source/face")
        if len(self.layout.qa) != len(record.qa):
            raise ValueError("Parsed Q&A evidence is incomplete")
        if self.content_hash != self.calculated_hash():
            raise ValueError("Parsed content hash mismatch")
        return self


class Card(RecordData):
    card_no: Text
    observations: tuple[ParsedObservation, ...]

    @model_validator(mode="after")
    def check_observations(self) -> Self:
        """Keep all source observations in deterministic, unambiguous order."""
        keys = tuple(o.evidence.descriptor.id for o in self.observations)
        if not keys or keys != tuple(sorted(set(keys))):
            raise ValueError("Parsed observations must be sorted and unique")
        if any(o.document.record.number != self.card_no for o in self.observations):
            raise ValueError("Parsed observation belongs to another card")
        return self


class FilePair(RecordData):
    format: Literal[1] = 1
    region: Region
    family: Family
    range: str
    cards: tuple[Card, ...]

    @model_validator(mode="after")
    def check_cards(self) -> Self:
        """Prevent duplicate cards and cross-region filing."""
        keys = tuple(c.card_no for c in self.cards)
        if not keys or keys != tuple(sorted(set(keys))):
            raise ValueError("Parsed cards must be sorted and unique")
        if any(
            o.document.region != self.region for c in self.cards for o in c.observations
        ):
            raise ValueError("Parsed pair region mismatch")
        return self


class FilePin(RecordData):
    path: Text
    sha256: Hash
    bytes: Annotated[int, Field(gt=0, le=MAX_BYTES)]
    evidence_sha256: Hash
    evidence_bytes: Annotated[int, Field(gt=0, le=MAX_BYTES)]
    cards: Annotated[int, Field(gt=0)]
    observations: Annotated[int, Field(gt=0)]

    @property
    def evidence_path(self) -> str:
        """Keep build evidence beside the readable card file with a fixed suffix."""
        return self.path.removesuffix(".json") + ".evidence.json"


class BatchSelection(RecordData):
    format: Literal[1] = 1
    id: Hash
    region: Region
    versions: dict[Version, bool]

    @model_validator(mode="after")
    def check_order(self) -> Self:
        """Keep selection maps stable without disclosing archive inventory metadata."""
        if not self.versions or list(self.versions) != sorted(self.versions):
            raise ValueError("Parsed batch versions must be nonempty and sorted")
        return self


class BatchFile(BatchSelection):
    part: UInt
    versions: Annotated[dict[Version, bool], Field(max_length=BATCH_PART_SIZE)]


class BatchPartPin(RecordData):
    sha256: Hash
    bytes: Annotated[int, Field(gt=0, le=MAX_BYTES)]


class BatchPin(RecordData):
    id: Hash
    parts: Annotated[tuple[BatchPartPin, ...], Field(min_length=1)]

    def path(self, part: int) -> str:
        """Give each immutable batch selection part a separate member."""
        return f"batches/{self.id[7:]}/{part:04d}.json"


class Index(RecordData):
    format: Literal[1] = 1
    recipe: Literal["sve-parsed-card-v1"] = RECIPE
    range_width: Literal[100] = RANGE_WIDTH
    files: tuple[FilePin, ...]
    batches: tuple[BatchPin, ...]

    @model_validator(mode="after")
    def check_order(self) -> Self:
        """Require a single deterministic order for the complete input closure."""
        for keys in (
            tuple(s.path for s in self.files),
            tuple(b.id for b in self.batches),
        ):
            if not keys or keys != tuple(sorted(set(keys))):
                raise ValueError("Parsed index entries must be sorted and unique")
        return self
