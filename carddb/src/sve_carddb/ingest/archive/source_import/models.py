"""Strict receipts; acquisition metadata never substitutes for adoption."""

import hashlib
from datetime import UTC, datetime
from pathlib import PurePosixPath
from typing import Annotated, Literal
from urllib.parse import urljoin, urlsplit

from pydantic import Field, field_validator, model_validator

from sve_carddb.core.json import canonical, digest, parse
from sve_carddb.core.models import Hash, RecordData, Text, UInt
from sve_carddb.core.provenance import FilePin, Revision
from sve_carddb.ingest.urls import canonicalize
from sve_carddb.registry.records import Region

_HOSTS = frozenset({"shadowverse-evolve.com", "en.shadowverse-evolve.com"})
_REDIRECTS = frozenset({301, 302, 303, 307, 308})
RawHash = Annotated[str, Field(pattern=r"^[0-9a-f]{64}\Z")]
type ImportKind = Literal["rules", "limit", "news"]


def official_url(value: str) -> str:
    """Validate before canonicalizing, so credentials or fragments cannot disappear."""
    parts = urlsplit(value)
    if (
        parts.scheme != "https"
        or parts.netloc not in _HOSTS
        or parts.fragment
        or any(char.isspace() or not char.isprintable() for char in value)
    ):
        raise ValueError("Source import requires an explicit official HTTPS URL")
    return canonicalize(value)


class Hop(RecordData):
    url: Text
    status: UInt
    location: str | None


class Observation(RecordData):
    url: Text
    final_url: Text
    status: Literal[200]
    sha256: RawHash
    bytes: Annotated[int, Field(gt=0, le=64 * 1024 * 1024)]
    fetched_at: Text
    content_type: Text
    etag: str | None
    last_modified: str | None
    chain: tuple[Hop, ...]

    @model_validator(mode="after")
    def acquisition(self) -> Observation:
        """Keep the requested identity and every redirect exactly as observed."""
        official_url(self.url)
        official_url(self.final_url)
        for hop in self.chain:
            official_url(hop.url)
        if (
            not self.chain
            or self.chain[0].url != self.url
            or self.chain[-1].url != self.final_url
            or self.chain[-1].status != self.status
            or self.chain[-1].location is not None
        ):
            raise ValueError("Source import requires a complete consistent HTTP chain")
        for previous, following in zip(self.chain, self.chain[1:], strict=False):
            if (
                previous.status not in _REDIRECTS
                or previous.location is None
                or urljoin(previous.url, previous.location) != following.url
            ):
                raise ValueError("Source import has an inconsistent redirect hop")
        if timestamp(self.fetched_at).utcoffset() != UTC.utcoffset(None):
            raise ValueError("Source import acquisition time must be UTC")
        if self.media_type not in {"text/html", "application/pdf"}:
            raise ValueError("Source import requires HTML or PDF media type")
        return self

    @property
    def media_type(self) -> str:
        """Classify without discarding the original Content-Type value."""
        return self.content_type.partition(";")[0].strip().lower()


class Purpose(RecordData):
    url: Text
    provider: Region
    kind: ImportKind

    @model_validator(mode="after")
    def identity(self) -> Purpose:
        """Classifications name canonical requested URLs, never final redirects."""
        if official_url(self.url) != self.url:
            raise ValueError("Source classification URL must be canonical")
        return self


class Selection(RecordData):
    source_import_format: Literal[1]
    sources: tuple[Purpose, ...]
    program_revision: Revision
    dependencies: tuple[FilePin, ...]

    @field_validator("source_import_format", mode="before")
    @classmethod
    def integer_format(cls, value: object) -> object:
        """Boolean one is not a format version."""
        if type(value) is not int:
            raise ValueError("Source import format must be integer 1")
        return value

    @model_validator(mode="after")
    def members(self) -> Selection:
        """Reject duplicates rather than silently shrinking the requested scope."""
        urls = tuple(item.url for item in self.sources)
        if not urls or len(urls) != len(set(urls)):
            raise ValueError("Source import selection must be nonempty and unique")
        check_dependencies(self.dependencies)
        return self


class Mapping(RecordData):
    url: Text
    provider: Region
    kind: ImportKind
    raw_hash: Hash
    path: Text


class Content(RecordData):
    source_mappings: tuple[Mapping, ...]
    program_revision: Revision
    dependencies: tuple[FilePin, ...]

    @model_validator(mode="after")
    def members(self) -> Content:
        """Require all canonical mappings in a deterministic order."""
        urls = tuple(item.url for item in self.source_mappings)
        if not urls or urls != tuple(sorted(set(urls))):
            raise ValueError(
                "Import receipt mappings must be nonempty, sorted and unique"
            )
        check_dependencies(self.dependencies)
        return self


def timestamp(value: str) -> datetime:
    """Reject date-only or naive clocks rather than inventing an event time."""
    result = datetime.fromisoformat(value)
    if "T" not in value or result.tzinfo is None:
        raise ValueError("Source import requires a complete timezone-aware instant")
    return result


def check_dependencies(values: tuple[FilePin, ...]) -> None:
    """Dependency pins are explicit portable metadata, with no implicit local defaults."""
    names = tuple(item.name for item in values)
    if not names or names != tuple(sorted(set(names))):
        raise ValueError(
            "Source import dependencies must be nonempty, sorted and unique"
        )


def load_index(raw: bytes) -> tuple[Observation, ...]:
    """Preserve exact index bytes while validating each complete JSONL observation."""
    rows = tuple(
        Observation.model_validate_json(canonical(parse(line)))
        for line in raw.splitlines()
    )
    urls = tuple(official_url(row.url) for row in rows)
    if not rows or len(urls) != len(set(urls)):
        raise ValueError("Source index must be nonempty with unique requested URLs")
    return rows


def raw_path(purpose: Purpose, observation: Observation) -> str:
    """Separate URL identities even when their raw content is identical."""
    suffix = "pdf" if observation.media_type == "application/pdf" else "html"
    name = hashlib.sha256(purpose.url.encode()).hexdigest()
    return PurePosixPath(
        "raw", purpose.provider, purpose.kind, name + "." + suffix
    ).as_posix()


def check_purpose(purpose: Purpose, observation: Observation) -> None:
    """Shared-host EN PDFs require an explicit region; HTML paths still match purpose."""
    path = urlsplit(purpose.url).path
    if observation.media_type == "application/pdf":
        if purpose.kind != "rules" or not path.lower().endswith(".pdf"):
            raise ValueError("PDF source import must be explicitly classified as rules")
        return
    host = (
        "shadowverse-evolve.com"
        if purpose.provider == "jp"
        else "en.shadowverse-evolve.com"
    )
    allowed = {
        "rules": path == "/rules/",
        "limit": path
        == ("/card_limit/" if purpose.provider == "jp" else "/restrictions/"),
        "news": path.startswith("/news/"),
    }
    if urlsplit(purpose.url).netloc != host or not allowed[purpose.kind]:
        raise ValueError("HTML source import URL does not match its explicit purpose")


def validate_content(index: bytes, content: Content) -> tuple[Observation, ...]:
    """Verify the entire resource mapping independently of the writing caller."""
    observations = {official_url(row.url): row for row in load_index(index)}
    if set(observations) != {item.url for item in content.source_mappings}:
        raise ValueError(
            "Import receipt mapping differs from the complete source index"
        )
    for mapping in content.source_mappings:
        purpose = Purpose(url=mapping.url, provider=mapping.provider, kind=mapping.kind)
        observation = observations[mapping.url]
        check_purpose(purpose, observation)
        if (
            mapping.raw_hash != "sha256:" + observation.sha256
            or mapping.path != raw_path(purpose, observation)
        ):
            raise ValueError(
                "Import receipt raw hash or path differs from the source index"
            )
    return tuple(observations[item.url] for item in content.source_mappings)


def receipt_id(index: bytes, content: Content) -> str:
    """The registration clock and private locations do not assign receipt identity."""
    return digest(
        canonical(
            {"index_sha256": digest(index), "content": content.model_dump(mode="json")}
        )
    )
