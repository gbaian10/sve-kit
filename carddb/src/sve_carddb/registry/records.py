"""Strict immutable data at the authored v1 build-input boundary."""

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field

from sve_carddb.build_db.domains import DATE, INSTANT

Text = Annotated[str, Field(min_length=1)]
Hash = Annotated[str, Field(pattern=r"^sha256:[0-9a-f]{64}\Z")]
Date = Annotated[str, Field(pattern="^" + DATE + "$")]
Instant = Annotated[str, Field(pattern="^" + INSTANT + "$")]
CardId = Annotated[str, Field(pattern=r"^c:[0-9a-f]{32}\Z")]
FaceId = Annotated[str, Field(pattern=r"^f:[0-9a-f]{32}\Z")]
PrintingId = Annotated[str, Field(pattern=r"^p:[0-9a-f]{32}\Z")]
UInt = Annotated[int, Field(ge=0, le=9007199254740991)]
Region = Literal["jp", "en"]
Recipe = Literal["registry-observation-v1"]


class RecordData(BaseModel):
    model_config = ConfigDict(
        extra="forbid", strict=True, frozen=True, regex_engine="python-re"
    )


class Observation(RecordData):
    region: Region
    card_no: Text
    recipe: Recipe
    observation_hash: Hash
    rules_hash: Hash


class CardData(RecordData):
    id: CardId
    layout: Literal["single", "double_faced"]
    identity_state: Literal["confirmed", "provisional", "retired"]
    home_set_id: Text


class FaceData(RecordData):
    id: FaceId
    card_id: CardId
    ordinal: UInt
    side: Literal["front", "back"]


class FaceMap(RecordData):
    source_index: UInt
    face_id: FaceId


class PrintingData(RecordData):
    id: PrintingId
    card_id: CardId
    region: Region
    card_no: Text
    variant_key: Text
    home_set_id: Text
    source_face_map: tuple[FaceMap, ...]
    observation: Observation


class CrossRegionReview(RecordData):
    checked: bool
    target_jp_card_no: Text | None
    target_observation: Observation | None


class EnglishPrintingData(PrintingData):
    cross_region_review: CrossRegionReview


class AllocationData(RecordData):
    int_id: Annotated[int, Field(ge=0, le=4294967295)]
    printing_id: PrintingId


class MappingReviewData(RecordData):
    card_id: CardId
    target_region: Region
    state: Literal["confirmed_none"]
    as_of: Date
    coverage_scope: Text
    coverage_hash: Hash
    observations: tuple[Observation, ...]


class ArtUse(RecordData):
    printing_id: PrintingId
    face_id: FaceId


class ArtData(RecordData):
    id: Annotated[str, Field(pattern=r"^a:[0-9a-f]{32}\Z")]
    card_id: CardId
    face_id: FaceId
    classification: Literal["base", "alternate", "unclassified"]
    uses: tuple[ArtUse, ...]
    observation: Observation


class RelatedEvidence(Observation):
    role: Literal["from", "to"]


class RelatedData(RecordData):
    id: Annotated[str, Field(pattern=r"^r:[0-9a-f]{32}\Z")]
    from_card_id: CardId
    to_card_id: CardId
    relation: Literal["same_rules_reskin"]
    source_kind: Literal["authored"]
    target_printing_id: None
    suggested_count: None
    dsl_id: None
    evidence: tuple[RelatedEvidence, ...]


class CorrectionEvidence(RecordData):
    kind: Literal["card_image"]
    sha256: Hash
    image_src: Text
    region: Region
    locator: Text


class CorrectionData(RecordData):
    id: Annotated[str, Field(pattern=r"^x:[0-9a-f]{32}\Z")]
    printing_id: PrintingId
    face_id: FaceId
    field: Literal["effect", "card_type"]
    expected_raw_value: str
    corrected_value: str
    expected_source_hash: Hash
    source_hash_recipe: Recipe
    reason: Text
    state: Literal["active", "needs_review"]
    reported_to_official: bool
    reported_on: Date | None
    report_url: str | None
    evidence: Annotated[tuple[CorrectionEvidence, ...], Field(min_length=1)]


class BatchDecision(RecordData):
    id: Annotated[str, Field(pattern=r"^d:[0-9a-f]{64}\Z")]
    state: Literal["confirmed", "proposed"]
    scope: Literal["batch"]
    category: Literal["identity_registry"]
    policy_id: Text
    membership_hash: Hash
    members: tuple[tuple[Text, Hash], ...]
    sample_ids: tuple[Text, ...]


DATA_MODELS: dict[str, type[RecordData]] = {
    "card": CardData,
    "face": FaceData,
    "printing": PrintingData,
    "card_int_id": AllocationData,
    "region_mapping_review": MappingReviewData,
    "art": ArtData,
    "card_related": RelatedData,
    "source_correction": CorrectionData,
}
