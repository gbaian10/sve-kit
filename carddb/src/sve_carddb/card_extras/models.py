"""Immutable shared input for sealed card pages and later incremental adapters."""

from typing import Literal
from urllib.parse import urlsplit

from pydantic import JsonValue, model_validator

from sve_carddb.build_inputs import Source  # ruff: ignore[typing-only-first-party-import] -- runtime Pydantic field
from sve_carddb.registry.records import Date, RecordData, Region, Text
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.sources import official_en, official_jp


def key(namespace: str, value: JsonValue) -> str:
    """Use content-addressed internal keys independently of official numbering."""
    return namespace + ":v1:" + digest(canonical(value)).removeprefix("sha256:")


class QAEntry(RecordData):
    stable_source_key: Text
    official_number: Text | None = None
    locator: Text
    question: Text
    answer: Text
    published_on: Date | None = None
    updated_on: Date | None = None
    date_raw: str | None = None
    state: Literal["active", "withdrawn"] = "active"

    def identity(self, region: Region) -> str:
        """Numbered questions share regional identity across every card page."""
        return key(
            "qa",
            [
                region,
                self.official_number,
                self.stable_source_key if self.official_number is None else None,
            ],
        )

    def fingerprint(self) -> str:
        """Dates describe a version; exact wording changes still create a new one."""
        return key(
            "qav",
            self.model_dump(
                mode="json", exclude={"locator", "stable_source_key", "official_number"}
            ),
        )


class RelatedLink(RecordData):
    locator: Text
    href_raw: Text


class CardPage(RecordData):
    source: Source
    region: Region
    card_no: Text
    qa: tuple[QAEntry, ...] = ()
    related: tuple[RelatedLink, ...] = ()
    errata_urls: tuple[Text, ...] = ()

    @model_validator(mode="after")
    def unique_locators(self) -> CardPage:
        """A parser may not silently overwrite two blocks with the same locator."""
        for items in (self.qa, self.related):
            locators = [item.locator for item in items]
            if len(set(locators)) != len(locators):
                raise ValueError("Duplicate card-page block locator")
        return self


class ErrataChange(RecordData):
    card_no: Text
    face_id: Text
    field: Text
    before_value: JsonValue
    after_value: JsonValue
    locator: Text


class ErrataPrinting(RecordData):
    card_no: Text
    scope: Literal["listed", "confirmed_applies"] = "listed"
    decision_id: Text | None = None

    @model_validator(mode="after")
    def confirmed_decision(self) -> ErrataPrinting:
        """A listing does not establish the printing's applicability."""
        if self.scope == "confirmed_applies" and self.decision_id is None:
            raise ValueError("Confirmed errata applicability requires a decision")
        return self


class ErrataPage(RecordData):
    source: Source
    region: Region
    official_url: Text
    announced_on: Date | None = None
    effective_on: Date | None = None
    date_raw: str | None = None
    reason: str | None = None
    exchange_offered: bool | None = None
    changes: tuple[ErrataChange, ...] = ()
    printings: tuple[ErrataPrinting, ...] = ()

    @model_validator(mode="after")
    def source_url(self) -> ErrataPage:
        """Keep announcement identity tied to its exact fetched source."""
        if self.official_url != self.source.url:
            raise ValueError("Errata URL differs from source URL")
        parts = urlsplit(self.official_url)
        host = official_jp.HOST if self.region == "jp" else official_en.HOST
        if (parts.scheme, parts.netloc) != ("https", host) or self.source.kind not in {
            "official_page",
            "official_pdf",
            "official_api",
        }:
            raise ValueError("Errata source region/media mismatch")
        return self
