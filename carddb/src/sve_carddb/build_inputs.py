"""Shared raw metadata and immutable, independently checkable build use records."""

from pathlib import PurePosixPath
from typing import TYPE_CHECKING, Annotated, Literal

from pydantic import Field, JsonValue, field_validator, model_validator

from sve_carddb.registry.records import Hash, Instant, RecordData, Text
from sve_carddb.snapshot.values import canonical, digest, parse

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping

    from sve_carddb.build_db import Database, Value

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

    def values(self) -> dict[str, Value]:
        """Project only version metadata; parser provenance belongs to each use."""
        return {
            "id": self.id,
            "kind": self.kind,
            "url": self.url,
            "raw_locator": self.raw_locator,
            "sha256": self.sha256,
            "fetched_at": self.fetched_at,
            "etag": self.etag,
            "last_modified": self.last_modified,
            "parser_version": None,
            "authored_path": None,
            "authored_revision": None,
        }


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
    dependencies: tuple[FilePin, ...]
    configuration: Text

    @model_validator(mode="after")
    def check_inputs(self) -> BuildContext:
        """Require explicit dependency pins and canonical immutable configuration."""
        names = tuple(pin.name for pin in self.dependencies)
        if not names or names != tuple(sorted(set(names))):
            raise ValueError("Dependency inputs must be nonempty, sorted and unique")
        if canonical(parse(self.configuration.encode())).decode() != self.configuration:
            raise ValueError("Build configuration must be canonical JSON")
        return self

    @classmethod
    def from_inputs(
        cls,
        program_revision: str,
        dependencies: Mapping[str, bytes],
        configuration: JsonValue,
    ) -> BuildContext:
        """Pin exact dependency bytes and an explicit configuration, without defaults."""
        return cls(
            program_revision=program_revision,
            dependencies=tuple(
                FilePin(name=name, sha256=digest(content))
                for name, content in sorted(dependencies.items())
            ),
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

    def verify(
        self,
        db: Database,
        context: BuildContext,
        expected: tuple[SourceUse, ...],
        *,
        complete: bool = True,
    ) -> None:
        """Compare against independently declared inputs and the actual database graph."""
        if self.context != context or self.uses != uses_sorted(expected):
            raise ValueError("Build input use closure or context mismatch")
        required = raw_values(use.source for use in expected)
        actual = {
            row.values["id"]: row.values
            for row in db.rows("source_record")
            if row.values["kind"] != "authored"
        }
        if (
            complete and actual.keys() != required.keys()
        ) or not required.keys() <= actual.keys():
            raise ValueError("Build input raw source closure mismatch")
        for source_id, values in required.items():
            if actual[source_id] != values:
                raise ValueError("Conflicting raw source metadata")


def input_record(context: BuildContext, uses: Iterable[SourceUse]) -> InputRecord:
    """Create a canonical record; callers must verify it against their input plans."""
    return InputRecord(context=context, uses=uses_sorted(uses))


def raw_values(sources: Iterable[Source]) -> dict[str, dict[str, Value]]:
    """Validate repeated version metadata before any database writes."""
    result: dict[str, dict[str, Value]] = {}
    for original in sources:
        source = Source.model_validate_json(original.model_dump_json())
        values = source.values()
        if source.id in result and result[source.id] != values:
            raise ValueError("Conflicting raw source metadata")
        result[source.id] = values
    return result


def insert_raw_sources(db: Database, sources: Iterable[Source]) -> None:
    """Reuse exact metadata only; never discard a conflicting version or its provenance."""
    required = raw_values(sources)
    for source_id, values in required.items():
        previous = db.select(
            "source_record", db.columns("source_record"), where={"id": source_id}
        )
        if not previous:
            db.insert("source_record", values)
        elif previous[0].values != values:
            raise ValueError("Conflicting raw source metadata")
