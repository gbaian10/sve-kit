"""Immutable product-authored-v1 wire types, independent of identity envelopes."""

from typing import Annotated, Literal, Self

from pydantic import Field, field_validator, model_validator

from sve_carddb.build_db.domains import CODE, DATE, INSTANT, LANG
from sve_carddb.registry.records import Hash, PrintingId, RecordData, Region, Text

Code = Annotated[str, Field(pattern="^" + CODE + r"\Z")]
Lang = Annotated[str, Field(pattern="^" + LANG + r"\Z")]
Date = Annotated[str, Field(pattern="^" + DATE + r"\Z")]
Instant = Annotated[str, Field(pattern="^" + INSTANT + r"\Z")]
Precision = Literal["day", "month", "year", "unknown"]


class LocalizedText(RecordData):
    lang: Lang
    text: str


class Language(RecordData):
    code: Lang
    fallback_order: tuple[Lang, ...]
    display_name: Text


class FamilyData(RecordData):
    id: Text
    code: Code
    public_code: Text
    kind: Literal[
        "booster", "promo", "deck", "collaboration", "special_pack", "special", "other"
    ]
    name: LocalizedText

    @model_validator(mode="after")
    def _name(self) -> FamilyData:
        if not self.name.text:
            raise ValueError("Family name must be nonempty")
        return self


def check_date(value: str | None, precision: Precision | None, raw: str | None) -> None:
    """Keep unknown/month/year and absent overrides distinct from full dates."""
    if precision == "day":
        if value is None:
            raise ValueError("Day precision requires a complete date")
    elif value is not None:
        raise ValueError("Only day precision permits an ISO date")
    if precision in {"month", "year"} and not raw:
        raise ValueError("Month/year precision requires original date text")
    if precision is None and raw is not None:
        raise ValueError("Absent date override requires all date fields to be null")


class ProductData(RecordData):
    id: Code
    region: Region
    family_id: Text | None
    product_code: str | None
    name: LocalizedText
    product_type: Code | None
    released_on: Date | None
    date_precision: Precision
    date_raw: str | None

    @model_validator(mode="after")
    def _dates(self) -> ProductData:
        if not self.name.text:
            raise ValueError("Product name must be nonempty")
        check_date(self.released_on, self.date_precision, self.date_raw)
        return self


class InclusionData(RecordData):
    printing_id: PrintingId
    product_id: Code
    first_available_on: Date | None
    first_available_precision: Precision | None
    first_available_raw: str | None
    inclusion_kind: Literal[
        "pack", "box", "first_edition_campaign", "qr_redemption", "event_prize", "other"
    ]
    note: LocalizedText | None

    @model_validator(mode="after")
    def _dates(self) -> InclusionData:
        check_date(
            self.first_available_on,
            self.first_available_precision,
            self.first_available_raw,
        )
        return self


class Evidence(RecordData):
    store_id: Text
    batch_id: Hash
    source_version_id: Annotated[str, Field(pattern=r"^src:v1:[0-9a-f]{64}\Z")]
    locator: Text
    role: Text

    @field_validator("store_id")
    @classmethod
    def _store(cls, value: str) -> str:
        if "/" in value or ".." in value:
            raise ValueError("Archive store ID must be a simple stable name")
        return value


class _Record(RecordData):
    record_key: Text
    filing_key: Annotated[str, Field(pattern=r"^[A-Za-z0-9_-]+\Z")]
    evidence: tuple[Evidence, ...]


class FamilyRecord(_Record):
    kind: Literal["product_family"]
    data: FamilyData


class AuthoredProductData(ProductData):
    product_type: Code


class ProductRecord(_Record):
    kind: Literal["product"]
    data: AuthoredProductData


class InclusionRecord(_Record):
    kind: Literal["printing_product"]
    data: InclusionData


CatalogRecord = Annotated[
    FamilyRecord | ProductRecord | InclusionRecord, Field(discriminator="kind")
]


class DecisionMetadata(RecordData):
    id: Annotated[str, Field(pattern=r"^d:[0-9a-f]{64}\Z")]
    state: Literal["proposed", "confirmed"]
    scope: Literal["batch"]
    membership_hash: Hash
    members: tuple[tuple[Text, Hash], ...]
    sample_ids: tuple[Text, ...]
    authored_by: Text
    authored_at: Instant
    reviewed_by: Text | None
    reviewed_at: Instant | None
    reviewed_precision: Literal["day", "instant"] | None
    note: str = ""

    @model_validator(mode="after")
    def _review(self) -> Self:
        if not self.authored_by.strip():
            raise ValueError("Authored author must be named")
        if self.state == "proposed":
            if self.sample_ids or any(
                value is not None
                for value in (
                    self.reviewed_by,
                    self.reviewed_at,
                    self.reviewed_precision,
                )
            ):
                raise ValueError("Proposed decisions must have no review metadata")
        elif (
            not self.reviewed_by
            or not self.reviewed_by.strip()
            or self.reviewed_at is None
            or self.reviewed_precision is None
        ):
            raise ValueError("Confirmed decisions require complete review metadata")
        if (
            self.reviewed_precision == "day"
            and self.reviewed_at is not None
            and not self.reviewed_at.endswith("T00:00:00Z")
        ):
            raise ValueError("Day review precision requires UTC midnight encoding")
        return self


class Decision(DecisionMetadata):
    category: Literal["product_catalog"]
    policy_id: Literal["product-authored-v1"]


class _Envelope(RecordData):
    product_authored_format: Literal[1]

    @field_validator("product_authored_format", mode="before")
    @classmethod
    def _version(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Product format must be an integer")
        return value


class Index(_Envelope):
    kind: Literal["product_index"]
    includes: dict[str, Hash]


class Shard(_Envelope):
    kind: Literal["product_shard"]
    default_decision_id: Text
    records: Annotated[tuple[CatalogRecord, ...], Field(min_length=1)]
    decisions: Annotated[tuple[Decision, ...], Field(min_length=1, max_length=1)]
