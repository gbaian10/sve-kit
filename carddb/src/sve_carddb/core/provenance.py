"""Shared raw metadata and summaries of actual build source uses."""

from datetime import date
from pathlib import PurePosixPath
from typing import TYPE_CHECKING, Annotated, Literal
from zoneinfo import ZoneInfo

from pydantic import Field, JsonValue, field_validator, model_validator

from sve_carddb.core.json import canonical, parse
from sve_carddb.core.models import Date, Hash, Instant, RecordData, Text

if TYPE_CHECKING:
    from collections.abc import Iterable


Revision = Annotated[str, Field(pattern=r"^[0-9a-f]{40}\Z")]
Version = Annotated[str, Field(pattern=r"^src:v1:[0-9a-f]{64}\Z")]
RawKind = Literal[
    "official_page",
    "official_api",
    "official_pdf",
    "image",
    "third_party_page",
    "third_party_audio",
]


class ArchivePin(RecordData):
    store_id: Text
    batch_id: Hash
    descriptor_sha256: Hash
    first_receipt_id: Hash


class Source(RecordData):
    id: Version
    kind: RawKind = "official_page"
    url: Text
    raw_locator: Text
    sha256: Hash
    fetched_at: Instant
    etag: str | None = None
    last_modified: str | None = None
    parser_version: Text
    archive: ArchivePin


class DateSource(RecordData):
    id: Version
    kind: Literal["official_page"]
    url: Text
    raw_locator: Text
    sha256: Hash
    fetched_at: None
    observed_precision: Literal["date"] = "date"
    observed_on: Date
    observed_timezone: Text
    observed_basis: Text
    import_receipt_id: Hash
    etag: str | None = None
    last_modified: str | None = None
    parser_version: Text
    archive: ArchivePin

    @model_validator(mode="after")
    def date_precision(self) -> DateSource:
        """Keep approved acquisition dates separate from instant-only sources."""
        date.fromisoformat(self.observed_on)
        try:
            ZoneInfo(self.observed_timezone)
        except (KeyError, ValueError) as error:
            raise ValueError(
                "Date source requires an IANA observation timezone"
            ) from error
        if not self.observed_basis.strip():
            raise ValueError("Date source needs its explicit acquisition basis")
        return self


class FilePin(RecordData):
    name: Text
    sha256: Hash

    @model_validator(mode="after")
    def check_name(self) -> FilePin:
        """Keep dependency input names portable and independent of private roots."""
        path = PurePosixPath(self.name)
        if path.is_absolute() or ".." in path.parts or path.as_posix() != self.name:
            raise ValueError("Dependency input name must be a canonical relative path")
        return self


class BuildContext(RecordData):
    program_revision: Revision
    configuration: Text

    @model_validator(mode="after")
    def check_inputs(self) -> BuildContext:
        """Require canonical configuration for the current build summary."""
        if canonical(parse(self.configuration.encode())).decode() != self.configuration:
            raise ValueError("Build configuration must be canonical JSON")
        return self

    @classmethod
    def from_inputs(
        cls,
        program_revision: str,
        configuration: JsonValue,
    ) -> BuildContext:
        """Describe the current program revision and explicit configuration."""
        return cls(
            program_revision=program_revision,
            configuration=canonical(configuration).decode(),
        )


class SourceUse(RecordData):
    source: Source
    usage: Text
    locator: Text


class DateSourceUse(RecordData):
    source: DateSource
    usage: Literal[
        "construction_profile", "construction_restriction", "construction_coverage"
    ]
    locator: Text


def _use_key(use: SourceUse) -> bytes:
    return canonical(use.model_dump(mode="json"))


def uses_sorted(uses: Iterable[SourceUse]) -> tuple[SourceUse, ...]:
    """Deduplicate exact uses while retaining distinct parsers, batches and locators."""
    return tuple(sorted(set(uses), key=_use_key))


class InputRecord(RecordData):
    input_format: Literal[1] = 1
    context: BuildContext
    uses: tuple[SourceUse, ...]
    date_uses: tuple[DateSourceUse, ...] = Field(
        default=(), exclude_if=lambda value: not value
    )

    @field_validator("input_format", mode="before")
    @classmethod
    def check_format(cls, value: object) -> object:
        """Validate before Literal coercion can turn a boolean into integer 1."""
        if type(value) is not int:
            raise ValueError("Build input format must be integer 1")
        return value

    @model_validator(mode="after")
    def check_uses(self) -> InputRecord:
        """Reject noncanonical inventories rather than silently normalizing them."""
        previous: bytes | None = None
        for use in self.uses:
            key = _use_key(use)
            if previous is not None and key <= previous:
                raise ValueError("Build uses must be sorted and unique")
            previous = key
        dated = [canonical(use.model_dump(mode="json")) for use in self.date_uses]
        if dated != sorted(set(dated)):
            raise ValueError("Build date uses must be sorted and unique")
        if {use.source.id for use in self.uses} & {
            use.source.id for use in self.date_uses
        }:
            raise ValueError("A source version cannot mix instant and date evidence")
        return self

    def content(self) -> bytes:
        """Serialize without card text or machine-local paths."""
        return canonical(self.model_dump(mode="json"))


def input_record(
    context: BuildContext,
    uses: Iterable[SourceUse],
    *,
    date_uses: Iterable[DateSourceUse] = (),
) -> InputRecord:
    """Summarize the sources consumed by this build."""
    instant = uses_sorted(uses)
    dated = tuple(
        sorted(set(date_uses), key=lambda use: canonical(use.model_dump(mode="json")))
    )
    if dated and {use.source.id for use in instant} & {use.source.id for use in dated}:
        raise ValueError("A source version cannot mix instant and date evidence")
    return InputRecord.model_construct(context=context, uses=instant, date_uses=dated)
