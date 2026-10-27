import hashlib
from itertools import pairwise
from typing import TYPE_CHECKING

import httpx
import pytest

from sve_carddb.core.regions import SourceRegion
from sve_carddb.ingest.archive.manifest import Kind, Manifest, Outcome
from sve_carddb.ingest.archive.store import decompress
from sve_carddb.ingest.crawl.construction_sources import (
    ConstructionSourceError,
    SourceResult,
    fetch_new,
    select_sources,
)
from sve_carddb.ingest.http.client import StopCrawlError
from sve_carddb.ingest.http.throttle import CircuitOpenError, Throttle
from sve_carddb.ingest.http.writer import PathConflictError, Writer
from sve_carddb.parse.html import MissingElementError
from sve_carddb.parse.pages.construction_outline import parse_construction_outline
from tests.support.manifest_history import request_outcomes

from ..conftest import FakeClock

if TYPE_CHECKING:
    from pathlib import Path

JP = "https://shadowverse-evolve.com/card_limit/"
EN = "https://en.shadowverse-evolve.com/news/post-145"
BODY = b"""<html><head><title>Synthetic restrictions</title></head><body>
<nav><a href="/navigation/">Outside article</a></nav><main>
<h1>Standard test outline</h1><p>Unreviewed synthetic content.</p>
<a href="/news/post-1">Announcement</a><a href="/rules/test.pdf">PDF</a>
<a href="https://example.invalid/other">Other link</a><img src="/unselected.png">
</main></body></html>"""


def test_explicit_source_classification_and_safe_paths() -> None:
    urls = [
        JP,
        EN,
        "https://en.shadowverse-evolve.com/restrictions/",
        "https://shadowverse-evolve.com/news/",
        "https://en.shadowverse-evolve.com/news/page/2/",
        "https://shadowverse-evolve.com/rules/crossover/",
        "https://en.shadowverse-evolve.com/rules/",
        JP,
    ]
    selected = select_sources(urls)
    assert [(source.region, source.kind, source.page_type) for source in selected] == [
        (SourceRegion.JP, Kind.LIMIT, "limit"),
        (SourceRegion.EN, Kind.NEWS, "notice"),
        (SourceRegion.EN, Kind.LIMIT, "limit"),
        (SourceRegion.JP, Kind.NEWS, "news_index"),
        (SourceRegion.EN, Kind.NEWS, "news_index"),
        (SourceRegion.JP, Kind.LIMIT, "limit"),
        (SourceRegion.EN, Kind.RULES, "rules"),
    ]
    assert len({source.path for source in selected}) == len(selected)
    assert all(not source.path.is_absolute() for source in selected)
    assert all(".." not in source.path.parts for source in selected)


@pytest.mark.parametrize(
    "url",
    [
        "http://shadowverse-evolve.com/card_limit/",
        "https://example.invalid/news/post-1",
        "https://en.shadowverse-evolve.com/card_limit/",
        "https://user@shadowverse-evolve.com/news/post-1",
        "https://shadowverse-evolve.com:443/news/post-1",
        "https://shadowverse-evolve.com/news/post-1?",
        "https://shadowverse-evolve.com/news/post-1#",
        "https://shadowverse-evolve.com/news/post-1\n",
        "https://shadowverse-evolve.com/news/%2e%2e/",
        "https://shadowverse-evolve.com/wordpress/test.pdf",
        "https://shadowverse-evolve.com/news/post-1/extra",
        "https://[invalid/news/post-1",
    ],
)
def test_rejects_unsafe_or_unclassified_selection(url: str) -> None:
    with pytest.raises(ConstructionSourceError):
        select_sources([JP, url])


def test_outline_keeps_all_content_hrefs_without_semantic_claims() -> None:
    outline = parse_construction_outline(BODY)
    assert outline.title == "Synthetic restrictions"
    assert outline.semantic_state == "unreviewed"
    assert [block.tag for block in outline.blocks] == ["h1", "p"]
    assert [link.href for link in outline.links] == [
        "/news/post-1",
        "/rules/test.pdf",
        "https://example.invalid/other",
    ]


@pytest.mark.parametrize(
    "body",
    [b"not HTML", b"<title>Test</title><main></main>", b"\xff"],
    ids=["not-html", "empty", "encoding"],
)
def test_outline_requires_supported_nonempty_html(body: bytes) -> None:
    with pytest.raises((ValueError, MissingElementError)):
        parse_construction_outline(body)


async def test_mock_batch_records_metadata_spaces_requests_and_skips_existing(
    tmp_path: Path,
) -> None:
    clock = FakeClock()
    calls: list[tuple[float, httpx.Request]] = []

    def respond(request: httpx.Request) -> httpx.Response:
        calls.append((clock.now, request))
        return httpx.Response(
            200,
            content=BODY,
            headers={"content-type": "text/html", "etag": '"synthetic"'},
        )

    reports: list[SourceResult] = []
    with Manifest.open(tmp_path / "manifest.sqlite") as manifest:
        writer = Writer(tmp_path / "data", manifest, create_only=True)
        async with httpx.AsyncClient(transport=httpx.MockTransport(respond)) as http:
            throttle = Throttle(2.5, 0, clock=clock, sleep=clock.sleep)
            assert (
                await fetch_new(
                    [JP],
                    http,
                    writer,
                    throttle=throttle,
                    run_id="synthetic",
                    report=reports.append,
                )
                == 0
            )
            assert (
                await fetch_new(
                    [JP, EN],
                    http,
                    writer,
                    throttle=throttle,
                    run_id="mixed",
                    report=reports.append,
                )
                == 0
            )
            before = tuple(manifest.resources.all())
            assert (
                await fetch_new(
                    [JP, EN],
                    http,
                    writer,
                    throttle=throttle,
                    run_id="skip",
                    report=reports.append,
                )
                == 0
            )
            assert tuple(manifest.resources.all()) == before
        assert [str(request.url) for _, request in calls] == [JP, EN]
        assert all(
            request.headers["user-agent"].startswith("Mozilla/") for _, request in calls
        )
        assert all(second[0] - first[0] >= 2 for first, second in pairwise(calls))
        for resource in before:
            assert resource.sha256 == hashlib.sha256(BODY).hexdigest()
            assert resource.etag == '"synthetic"'
            assert resource.first_fetched_at is not None
            assert decompress((tmp_path / "data" / resource.path).read_bytes()) == BODY
            assert request_outcomes(manifest, resource.url) == [Outcome.CHANGED]
        assert not manifest.requests.has_started()
    assert [report.outcome for report in reports] == [
        "fetched",
        "existing-trusted",
        "fetched",
        "existing-trusted",
        "existing-trusted",
    ]


@pytest.mark.parametrize(
    ("status", "body"),
    [
        (200, b"<title>Challenge</title>"),
        (304, BODY),
        (404, BODY),
        (503, BODY),
        (200, b"<html><head><title>T</title></head><body><main><h1>x</h1><p>abc"),
    ],
    ids=["incomplete", "not-modified", "missing", "unavailable", "truncated"],
)
async def test_invalid_response_is_not_stored_or_retried(
    tmp_path: Path,
    status: int,
    body: bytes,
) -> None:
    calls: list[httpx.Request] = []

    def respond(request: httpx.Request) -> httpx.Response:
        calls.append(request)
        return httpx.Response(
            status,
            content=body,
            headers={"content-type": "text/html"},
        )

    with Manifest.open(tmp_path / "manifest.sqlite") as manifest:
        async with httpx.AsyncClient(transport=httpx.MockTransport(respond)) as http:
            assert (
                await fetch_new(
                    [JP],
                    http,
                    Writer(tmp_path / "data", manifest, create_only=True),
                    throttle=Throttle(2, 0),
                    run_id="invalid",
                    report=lambda _: None,
                )
                == 1
            )
        assert manifest.resources.get(JP) is None
        assert request_outcomes(manifest, JP) == [Outcome.FAILED]
        assert not manifest.requests.has_started()
    assert len(calls) == 1


@pytest.mark.parametrize("status", [403, 429])
async def test_error_status_never_retries(tmp_path: Path, status: int) -> None:
    calls: list[httpx.Request] = []

    def respond(request: httpx.Request) -> httpx.Response:
        calls.append(request)
        return httpx.Response(status, content=BODY)

    with Manifest.open(tmp_path / "manifest.sqlite") as manifest:
        async with httpx.AsyncClient(transport=httpx.MockTransport(respond)) as http:

            async def fetch() -> int:
                return await fetch_new(
                    [JP],
                    http,
                    Writer(tmp_path / "data", manifest, create_only=True),
                    throttle=Throttle(2, 0),
                    run_id="status",
                    report=lambda _: None,
                )

            if status == 403:
                with pytest.raises(StopCrawlError):
                    await fetch()
            else:
                assert await fetch() == 1
        assert manifest.resources.get(JP) is None
        assert request_outcomes(manifest, JP) == [Outcome.FAILED]
        assert not manifest.requests.has_started()
    assert len(calls) == 1


async def test_existing_destination_blocks_all_requests(tmp_path: Path) -> None:
    calls: list[httpx.Request] = []
    source = select_sources([JP])[0]
    destination = tmp_path / "data" / source.path
    destination.parent.mkdir(parents=True)
    destination.write_bytes(b"unowned")

    def respond(request: httpx.Request) -> httpx.Response:
        calls.append(request)
        return httpx.Response(200, content=BODY)

    with Manifest.open(tmp_path / "manifest.sqlite") as manifest:
        async with httpx.AsyncClient(transport=httpx.MockTransport(respond)) as http:
            with pytest.raises(PathConflictError):
                await fetch_new(
                    [EN, JP],
                    http,
                    Writer(tmp_path / "data", manifest, create_only=True),
                    throttle=Throttle(2, 0),
                    run_id="destination",
                    report=lambda _: None,
                )
        assert not manifest.requests.has_started()
        assert list(manifest.resources.all()) == []
    assert calls == []
    assert destination.read_bytes() == b"unowned"


async def test_refuses_redirect_even_to_another_selected_url(tmp_path: Path) -> None:
    calls: list[httpx.Request] = []

    def respond(request: httpx.Request) -> httpx.Response:
        calls.append(request)
        return httpx.Response(302, headers={"location": EN})

    with Manifest.open(tmp_path / "manifest.sqlite") as manifest:
        async with httpx.AsyncClient(transport=httpx.MockTransport(respond)) as http:
            with pytest.raises(StopCrawlError, match="redirect refused"):
                await fetch_new(
                    [JP, EN],
                    http,
                    Writer(tmp_path / "data", manifest, create_only=True),
                    throttle=Throttle(2, 0),
                    run_id="redirect",
                    report=lambda _: None,
                )
        assert request_outcomes(manifest, JP) == [Outcome.REDIRECTED]
        assert list(manifest.resources.all()) == []
    assert [str(request.url) for request in calls] == [JP]


async def test_invalid_selection_and_writer_stop_before_requests(
    tmp_path: Path,
) -> None:
    calls: list[httpx.Request] = []

    def respond(request: httpx.Request) -> httpx.Response:
        calls.append(request)
        return httpx.Response(200, content=BODY)

    with Manifest.open(tmp_path / "manifest.sqlite") as manifest:
        async with httpx.AsyncClient(transport=httpx.MockTransport(respond)) as http:
            with pytest.raises(ConstructionSourceError):
                await fetch_new(
                    [JP, "https://example.invalid/news/"],
                    http,
                    Writer(tmp_path / "data", manifest, create_only=True),
                    throttle=Throttle(2, 0),
                    run_id="invalid",
                    report=lambda _: None,
                )
            with pytest.raises(StopCrawlError, match="create-only"):
                await fetch_new(
                    [JP],
                    http,
                    Writer(tmp_path / "data", manifest),
                    throttle=Throttle(2, 0),
                    run_id="writer",
                    report=lambda _: None,
                )
    assert calls == []


async def test_untrusted_existing_source_prevents_the_entire_batch(
    tmp_path: Path,
) -> None:
    calls: list[httpx.Request] = []

    def respond(request: httpx.Request) -> httpx.Response:
        calls.append(request)
        return httpx.Response(200, content=BODY, headers={"content-type": "text/html"})

    clock = FakeClock()
    with Manifest.open(tmp_path / "manifest.sqlite") as manifest:
        writer = Writer(tmp_path / "data", manifest, create_only=True)
        async with httpx.AsyncClient(transport=httpx.MockTransport(respond)) as http:
            await fetch_new(
                [JP],
                http,
                writer,
                throttle=Throttle(2, 0, clock=clock, sleep=clock.sleep),
                run_id="seed",
                report=lambda _: None,
            )
            resource = manifest.resources.get(JP)
            assert resource is not None
            (tmp_path / "data" / resource.path).write_bytes(b"corrupt")
            calls.clear()
            with pytest.raises(StopCrawlError, match="not trusted"):
                await fetch_new(
                    [EN, JP],
                    http,
                    writer,
                    throttle=Throttle(2, 0, clock=clock, sleep=clock.sleep),
                    run_id="untrusted",
                    report=lambda _: None,
                )
        assert manifest.resources.get(EN) is None
        assert calls == []


async def test_non_html_mime_is_not_admitted(tmp_path: Path) -> None:
    with Manifest.open(tmp_path / "manifest.sqlite") as manifest:
        async with httpx.AsyncClient(
            transport=httpx.MockTransport(
                lambda _: httpx.Response(
                    200, content=BODY, headers={"content-type": "application/pdf"}
                )
            )
        ) as http:
            assert (
                await fetch_new(
                    [JP],
                    http,
                    Writer(tmp_path / "data", manifest, create_only=True),
                    throttle=Throttle(2, 0),
                    run_id="mime",
                    report=lambda _: None,
                )
                == 1
            )
        assert manifest.resources.get(JP) is None
        assert request_outcomes(manifest, JP) == [Outcome.FAILED]
        assert not manifest.requests.has_started()


async def test_consecutive_failures_open_the_circuit(tmp_path: Path) -> None:
    calls: list[httpx.Request] = []
    urls = [f"https://en.shadowverse-evolve.com/news/post-{n}" for n in range(1, 6)]

    def respond(request: httpx.Request) -> httpx.Response:
        calls.append(request)
        return httpx.Response(404, content=BODY, headers={"content-type": "text/html"})

    with Manifest.open(tmp_path / "manifest.sqlite") as manifest:
        async with httpx.AsyncClient(transport=httpx.MockTransport(respond)) as http:
            with pytest.raises(CircuitOpenError):
                await fetch_new(
                    urls,
                    http,
                    Writer(tmp_path / "data", manifest, create_only=True),
                    throttle=Throttle(2, 0),
                    run_id="circuit",
                    report=lambda _: None,
                )
        assert not manifest.requests.has_started()
    assert len(calls) == 3
