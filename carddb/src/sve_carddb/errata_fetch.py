"""Fetch a reviewed JP errata URL selection without replacing or recovering sources."""

import json
import re
from dataclasses import dataclass
from pathlib import PurePosixPath
from typing import TYPE_CHECKING
from urllib.parse import unquote, urlsplit

from sve_carddb.fetch.client import Client, FetchError, Request, StopCrawlError
from sve_carddb.fetch.validate import ValidationError, require_media_type
from sve_carddb.fetch.writer import Fetched, LocalState, Writer, sha256
from sve_carddb.html import parse, select_one
from sve_carddb.manifest import Kind, Manifest, Outcome, Region, RequestResult
from sve_carddb.urls import canonicalize

if TYPE_CHECKING:
    from collections.abc import Callable, Sequence
    from pathlib import Path

    from sve_carddb.config import Settings
    from sve_carddb.fetch.client import Response
    from sve_carddb.fetch.throttle import CircuitBreaker

_ANNOUNCEMENT = re.compile(r"/errata/([^/]+)/?\Z")
_OK = 200


class ErrataInputError(ValueError):
    """The selection is not an explicit list of safe JP errata URLs."""


def validate_urls(values: Sequence[str]) -> tuple[str, ...]:
    """Validate the entire selection before opening a manifest or making requests."""
    if not values:
        raise ErrataInputError("errata URL selection is empty")
    result: dict[str, None] = {}
    for value in values:
        parts = urlsplit(value)
        match = _ANNOUNCEMENT.fullmatch(parts.path)
        if (
            any(not char.isprintable() or char.isspace() for char in value)
            or (parts.scheme, parts.netloc) != ("https", "shadowverse-evolve.com")
            or any(delimiter in value for delimiter in "?#")
            or match is None
        ):
            raise ErrataInputError("selection contains a non-JP-errata URL")
        slug = unquote(match[1], errors="strict")
        if slug in {".", ".."} or any(
            not char.isprintable() or char.isspace() or char in "/\\" for char in slug
        ):
            raise ErrataInputError("selection contains an unsafe errata path")
        result[canonicalize(value)] = None
    return tuple(result)


def load_urls(path: Path) -> tuple[str, ...]:
    """Load a JSON array of explicit URL strings; no discovery or embedded instructions."""
    value: object = json.loads(path.read_bytes())
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise ErrataInputError("errata URL file must be a JSON array of strings")
    return validate_urls(value)


def raw_path(url: str) -> PurePosixPath:
    """Use a canonical URL digest, avoiding slug-derived filenames and collisions."""
    return PurePosixPath("raw", "jp", "errata", f"{sha256(url.encode())}.html.zst")


def require_quiet(settings: Settings, manifest: Manifest) -> None:
    """Refuse unfinished work; never clean files or mutate historical request outcomes."""
    if manifest.requests.has_started():
        raise StopCrawlError("unfinished manifest requests require separate recovery")
    if next(settings.data_dir.rglob("*.tmp-*"), None) is not None:
        raise StopCrawlError("unfinished raw temporary files require separate recovery")
    root = settings.archive_root
    if root is not None:
        for directory, markers in (
            ("replacements", "replacement-completions"),
            ("observations", "observation-seals"),
        ):
            if any(
                not (root / markers / path.name).is_file()
                for path in (root / directory).glob("*.json")
            ):
                raise StopCrawlError(
                    "unfinished archive work requires separate recovery"
                )
        if next((root / "staging").glob("*"), None) is not None:
            raise StopCrawlError("archive staging requires separate recovery")


def pending_urls(writer: Writer, urls: Sequence[str]) -> tuple[str, ...]:
    """Preflight all sources, skipping trusted existing copies without metadata updates."""
    pending: list[str] = []
    for url in urls:
        resource = writer.manifest.resources.get(url)
        if resource is not None:
            if (
                resource.region is not Region.JP
                or resource.kind is not Kind.ERRATA
                or writer.local_state(url) is not LocalState.TRUSTED
            ):
                raise StopCrawlError(f"existing errata source is not trusted: {url}")
        else:
            writer.check_new(url, raw_path(url))
            pending.append(url)
    return tuple(pending)


def _validate_body(response: Response) -> None:
    require_media_type(response.content_type, "text/html")
    document = parse(response.body.decode("utf-8"))
    title = select_one(document, "title")
    body = select_one(document, "main, article, .entry-content")
    if (
        title is None
        or not title.text().strip()
        or body is None
        or not body.text().strip()
    ):
        raise ValidationError("HTML lacks a title or announcement body container")


@dataclass(frozen=True, slots=True)
class ErrataResult:
    """Metadata-only progress for a single URL; never contains announcement text."""

    url: str
    outcome: str
    raw_sha256: str | None = None
    raw_bytes: int | None = None


async def fetch_new(
    urls: Sequence[str],
    client: Client,
    writer: Writer,
    breaker: CircuitBreaker,
    report: Callable[[ErrataResult], None],
) -> int:
    """Fetch only new URLs from a fully validated selection, reporting each result."""
    selection = validate_urls(urls)
    if not writer.create_only:
        raise StopCrawlError("errata fetch requires a create-only writer")
    pending = frozenset(pending_urls(writer, selection))
    failures = 0
    for url in selection:
        if url not in pending:
            report(ErrataResult(url, "existing-trusted"))
            continue
        writer.check_new(url, raw_path(url))
        try:
            # The selection remains the only network allowlist, including redirect hops.
            response = await client.get(Request(url, allowed=selection.__contains__))
        except FetchError:
            failures += 1
            report(ErrataResult(url, "failed"))
            breaker.record_failure("errata fetch failed")
            continue
        if not _acceptable(response, writer.manifest):
            failures += 1
            report(ErrataResult(url, "failed-validation"))
            breaker.record_failure("errata response failed validation")
            continue
        result = writer.write(
            Fetched(
                url=url,
                region=Region.JP,
                kind=Kind.ERRATA,
                path=raw_path(url),
                body=response.body,
                content_type=response.content_type or "",
                etag=response.etag,
                last_modified=response.last_modified,
                compressed=True,
            ),
            request_id=response.request_id,
        )
        breaker.record_success()
        report(
            ErrataResult(
                url, "fetched", result.resource.sha256, result.resource.raw_bytes
            )
        )
    return failures


def _acceptable(response: Response, manifest: Manifest) -> bool:
    reason: str | None = None
    if response.status != _OK:
        reason = "new source requires HTTP 200"
    else:
        try:
            _validate_body(response)
        except ValidationError, UnicodeError:
            reason = "invalid errata HTML"
    if reason is None:
        return True
    with manifest.transaction():
        manifest.requests.finish(
            response.request_id,
            RequestResult(
                Outcome.FAILED,
                final_url=response.url,
                status=response.status,
                validation_error=reason,
            ),
        )
    return False
