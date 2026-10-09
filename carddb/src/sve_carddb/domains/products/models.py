"""Immutable product-authored-v1 wire types, independent of identity shards."""

from typing import Annotated, Literal

from pydantic import Field, computed_field, field_validator, model_validator

from sve_carddb.build.scalars import CODE, LANG
from sve_carddb.core.dates import DATE, INSTANT
from sve_carddb.core.json import canonical
from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.core.regions import Region
from sve_carddb.domains.registry.records import PrintingId

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
    batch_id: Hash
    source_version_id: Annotated[str, Field(pattern=r"^src:v1:[0-9a-f]{64}\Z")]
    locator: Text
    role: Text


class _Record(RecordData):
    filing_key: Annotated[str, Field(pattern=r"^[A-Za-z0-9_-]+\Z")]
    # Proposed records stay candidates; only confirmed ones are projected.
    state: Literal["proposed", "confirmed"]
    note: str = ""
    evidence: tuple[Evidence, ...]


class FamilyRecord(_Record):
    kind: Literal["product_family"]
    data: FamilyData

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes this property; mypy cannot compose property decorators.
    @property
    def record_key(self) -> str:
        """Derive identity independently of mutable values and authored metadata."""
        return canonical([self.kind, self.data.id]).decode()


class AuthoredProductData(ProductData):
    product_type: Code


class ProductRecord(_Record):
    kind: Literal["product"]
    data: AuthoredProductData

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes this property; mypy cannot compose property decorators.
    @property
    def record_key(self) -> str:
        """Derive identity independently of mutable values and authored metadata."""
        return canonical([self.kind, self.data.id]).decode()


class InclusionRecord(_Record):
    kind: Literal["printing_product"]
    data: InclusionData

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes this property; mypy cannot compose property decorators.
    @property
    def record_key(self) -> str:
        """Derive identity independently of mutable values and authored metadata."""
        return canonical(
            [self.kind, self.data.printing_id, self.data.product_id]
        ).decode()


CatalogRecord = Annotated[
    FamilyRecord | ProductRecord | InclusionRecord, Field(discriminator="kind")
]


class Shard(RecordData):
    format: Literal[1]
    kind: Literal["product_shard"]
    records: Annotated[tuple[CatalogRecord, ...], Field(min_length=1)]

    @field_validator("format", mode="before")
    @classmethod
    def _version(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Product format must be an integer")
        return value
