"""Explicit, create-only construction source batches; no discovery or live CLI."""

import json
import re
from dataclasses import dataclass
from pathlib import PurePosixPath
from typing import TYPE_CHECKING, Literal
from urllib.parse import urlsplit

from sve_carddb.core.regions import SourceRegion
from sve_carddb.ingest.archive.manifest import Kind, Outcome, RequestResult
from sve_carddb.ingest.config import BROWSER_USER_AGENT
from sve_carddb.ingest.http.client import (
    Client,
    ClientPolicy,
    FetchError,
    Request,
    StopCrawlError,
)
from sve_carddb.ingest.http.throttle import CircuitBreaker
from sve_carddb.ingest.http.validate import require_media_type
from sve_carddb.ingest.http.writer import Fetched, LocalState, sha256
from sve_carddb.ingest.urls import canonicalize
from sve_carddb.parse.html import MissingElementError
from sve_carddb.parse.pages.construction_outline import parse_construction_outline

if TYPE_CHECKING:
    from collections.abc import Callable, Sequence
    from pathlib import Path

    import httpx

    from sve_carddb.ingest.http.client import Response
    from sve_carddb.ingest.http.throttle import Throttle
    from sve_carddb.ingest.http.writer import Writer

type PageType = Literal["limit", "news_index", "notice", "rules"]
_REGIONS = {
    "shadowverse-evolve.com": SourceRegion.JP,
    "en.shadowverse-evolve.com": SourceRegion.EN,
}
_LIMITS = {
    (SourceRegion.JP, "/card_limit/"),
    (SourceRegion.JP, "/rules/crossover/"),
    (SourceRegion.EN, "/restrictions/"),
}
_NOTICE = re.compile(r"/news/post-[1-9][0-9]*/?\Z")
_INDEX = re.compile(r"/news/(?:page/[1-9][0-9]*/)?\Z")
_OK = 200
_BREAKER_THRESHOLD = 3


class ConstructionSourceError(ValueError):
    """A batch includes an unsupported or unsafe construction source URL."""


@dataclass(frozen=True, slots=True)
class ConstructionSource:
    url: str
    region: SourceRegion
    kind: Kind
    page_type: PageType
    path: PurePosixPath


def select_sources(urls: Sequence[str]) -> tuple[ConstructionSource, ...]:
    """Validate the whole explicit batch before any manifest or HTTP operations."""
    if not urls:
        raise ConstructionSourceError("construction source selection is empty")
    selected: dict[str, ConstructionSource] = {}
    for url in urls:
        try:
            parts = urlsplit(url)
        except ValueError as exc:
            raise ConstructionSourceError("selection contains a malformed URL") from exc
        if (
            any(not char.isprintable() or char.isspace() for char in url)
            or parts.scheme != "https"
            or parts.netloc not in _REGIONS
            or any(delimiter in url for delimiter in "?#")
        ):
            raise ConstructionSourceError("selection contains an unsafe source URL")
        region = _REGIONS[parts.netloc]
        page_type: PageType
        if (region, parts.path) in _LIMITS:
            kind, page_type = Kind.LIMIT, "limit"
        elif _NOTICE.fullmatch(parts.path):
            kind, page_type = Kind.NEWS, "notice"
        elif _INDEX.fullmatch(parts.path):
            kind, page_type = Kind.NEWS, "news_index"
        elif parts.path == "/rules/":
            kind, page_type = Kind.RULES, "rules"
        else:
            raise ConstructionSourceError(
                "selection contains an unsupported source path"
            )
        canonical = canonicalize(url)
        path = PurePosixPath(
            "raw", region.value, kind.value, f"{sha256(canonical.encode())}.html.zst"
        )
        selected[canonical] = ConstructionSource(
            canonical, region, kind, page_type, path
        )
    return tuple(selected.values())


def load_sources(path: Path) -> tuple[ConstructionSource, ...]:
    """Read a URL array, never an executable discovery recipe."""
    value: object = json.loads(path.read_bytes())
    if not isinstance(value, list) or not all(isinstance(url, str) for url in value):
        raise ConstructionSourceError("selection must be a JSON array of URL strings")
    return select_sources(value)


@dataclass(frozen=True, slots=True)
class SourceResult:
    url: str
    outcome: Literal["existing-trusted", "fetched", "failed"]
    raw_sha256: str | None = None


def _validate_response(response: Response) -> None:
    if response.status != _OK:
        msg = "new construction source requires HTTP 200"
        raise ValueError(msg)
    require_media_type(response.content_type, "text/html")
    parse_construction_outline(response.body)


async def fetch_new(
    urls: Sequence[str],
    http: httpx.AsyncClient,
    writer: Writer,
    *,
    throttle: Throttle,
    run_id: str,
    report: Callable[[SourceResult], None],
) -> int:
    """Use caller-owned locking/storage; never retry, redirect, refresh or follow hrefs."""
    selection = select_sources(urls)
    if not writer.create_only:
        raise StopCrawlError("construction sources require a create-only writer")
    pending: list[ConstructionSource] = []
    for source in selection:
        existing = writer.manifest.resources.get(source.url)
        if existing is None:
            writer.check_new(source.url, source.path)
            pending.append(source)
        elif (
            existing.region is not source.region
            or existing.kind is not source.kind
            or existing.path != source.path
            or writer.local_state(source.url) is not LocalState.TRUSTED
        ):
            raise StopCrawlError("existing construction source is not trusted")
    allowed = frozenset(source.url for source in selection)
    client = Client(
        http,
        throttle,
        writer.manifest,
        run_id=run_id,
        policy=ClientPolicy(
            attempts=1, max_requests=len(pending), stop_on_redirect=True
        ),
    )
    pending_urls = {source.url for source in pending}
    breaker = CircuitBreaker(_BREAKER_THRESHOLD)
    failures = 0
    for source in selection:
        if source.url not in pending_urls:
            report(SourceResult(source.url, "existing-trusted"))
            continue
        try:
            response = await client.get(
                Request(
                    source.url,
                    allowed=allowed.__contains__,
                    headers=(("User-Agent", BROWSER_USER_AGENT),),
                )
            )
        except FetchError:
            failures += 1
            report(SourceResult(source.url, "failed"))
            breaker.record_failure("construction source fetch failed")
            continue
        try:
            _validate_response(response)
        except ValueError, MissingElementError:
            with writer.manifest.transaction():
                writer.manifest.requests.finish(
                    response.request_id,
                    RequestResult(
                        Outcome.FAILED,
                        final_url=response.url,
                        status=response.status,
                        validation_error="unverified construction HTML structure",
                    ),
                )
            failures += 1
            report(SourceResult(source.url, "failed"))
            breaker.record_failure("construction source failed validation")
            continue
        result = writer.write(
            Fetched(
                source.url,
                source.region,
                source.kind,
                source.path,
                response.body,
                response.content_type or "",
                response.etag,
                response.last_modified,
                compressed=True,
            ),
            request_id=response.request_id,
        )
        breaker.record_success()
        report(SourceResult(source.url, "fetched", result.resource.sha256))
    return failures
