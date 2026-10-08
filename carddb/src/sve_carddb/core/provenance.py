"""Shared raw metadata and summaries of actual build source uses."""

from pathlib import PurePosixPath
from typing import TYPE_CHECKING, Annotated, Literal

from pydantic import Field, JsonValue, field_validator, model_validator

from sve_carddb.core.json import canonical, parse
from sve_carddb.core.models import Hash, Instant, RecordData, Text

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


def _use_key(use: SourceUse) -> bytes:
    return canonical(use.model_dump(mode="json"))


def uses_sorted(uses: Iterable[SourceUse]) -> tuple[SourceUse, ...]:
    """Deduplicate exact uses while retaining distinct parsers, batches and locators."""
    return tuple(sorted(set(uses), key=_use_key))


class InputRecord(RecordData):
    input_format: Literal[1] = 1
    context: BuildContext
    uses: tuple[SourceUse, ...]

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
        if self.uses != uses_sorted(self.uses):
            raise ValueError("Build uses must be sorted and unique")
        return self

    def content(self) -> bytes:
        """Serialize without card text or machine-local paths."""
        return canonical(self.model_dump(mode="json"))


def input_record(context: BuildContext, uses: Iterable[SourceUse]) -> InputRecord:
    """Summarize the sources consumed by this build."""
    return InputRecord(context=context, uses=uses_sorted(uses))
