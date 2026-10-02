import errno
import json
import os
import re
from dataclasses import dataclass, field, replace
from functools import partial
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from itertools import pairwise
from threading import Thread
from typing import TYPE_CHECKING, override

import httpx
import pytest
import stamina
from typer.testing import CliRunner

from sve_carddb import cli
from sve_carddb.errata_fetch import (
    ErrataInputError,
    _validate_body,
    fetch_new,
    load_urls,
    raw_path,
    url_region,
    validate_urls,
)
from sve_carddb.fetch.client import (
    BudgetExhaustedError,
    Client,
    Request,
    Response,
    StopCrawlError,
)
from sve_carddb.fetch.throttle import CircuitBreaker, Throttle
from sve_carddb.fetch.validate import ValidationError
from sve_carddb.fetch.writer import PathConflictError, Writer, sha256
from sve_carddb.manifest import (
    ExclusiveLock,
    Kind,
    Manifest,
    Outcome,
    Region,
    RequestResult,
    RequestStart,
)
from sve_carddb.store import UnsafePathError, decompress

from .conftest import FakeClock

if TYPE_CHECKING:
    from collections.abc import Callable, Iterator
    from pathlib import Path

    from typer.testing import Result

    from sve_carddb.config import Settings
    from sve_carddb.errata_fetch import ErrataResult

HOST = "https://shadowverse-evolve.com"
URL = f"{HOST}/errata/synthetic-one/"
OTHER = f"{HOST}/errata/synthetic-two/"
EN_HOST = "https://en.shadowverse-evolve.com"
EN_URL = f"{EN_HOST}/errata/synthetic-one/"
EN_OTHER = f"{EN_HOST}/errata/synthetic-two/"


def notice_body(region: Region = Region.JP) -> bytes:
    prefix = "eratta" if region is Region.JP else "errata"
    return (
        "<html><head><title>Synthetic notice</title></head><body>"
        '<div class="st-Container"><div class="st-Container_Inner">'
        '<div class="sw-Lower"><div class="sw-Lower_Wrapper">'
        '<div class="sw-Lower_Heading"><h1 class="sw-Ttl">Synthetic errata</h1></div>'
        f'<div class="sw-Lower_Container"><div class="{prefix}-Detail">'
        f'<div class="{prefix}-Detail_Inner"><div class="heading">'
        '<h1 class="ttl">Synthetic correction</h1></div>'
        '<div class="contents sw-Txtarea">Synthetic body'
        '<a href="/rules/">Unselected source</a><img src="/image.png"></div>'
        "</div></div></div></div></div></div></div></body></html>"
    ).encode()


BODY = notice_body()
runner = CliRunner()


def ok(region: Region = Region.JP) -> httpx.Response:
    return httpx.Response(
        200,
        content=notice_body(region),
        headers={
            "content-type": "text/html; charset=utf-8",
            "etag": '"synthetic"',
            "last-modified": "Thu, 01 Oct 2026 00:00:00 GMT",
        },
    )


@dataclass
class FakeServer:
    clock: FakeClock
    responses: list[httpx.Response | BaseException] = field(default_factory=list)
    calls: list[tuple[float, httpx.Request]] = field(default_factory=list)

    def __call__(self, request: httpx.Request) -> httpx.Response:
        self.calls.append((self.clock.now, request))
        response = (
            self.responses.pop(0)
            if self.responses
            else ok(url_region(str(request.url)))
        )
        if isinstance(response, BaseException):
            raise response
        return response


@dataclass
class Case:
    root: Path
    urls_file: Path
    server: FakeServer

    def invoke(
        self, urls: tuple[str, ...] = (URL,), *, dry_run: bool = False
    ) -> Result:
        # Fail closed if a patch restoration ever removes the isolated data root.
        assert os.environ.get("SVE_DATA_DIR") == str(self.root)
        self.urls_file.write_text(json.dumps(urls))
        args = ["crawl", "errata-new", "--urls", str(self.urls_file)]
        if dry_run:
            args.append("--dry-run")
        return runner.invoke(cli.app, args)

    def manifest(self) -> Manifest:
        return Manifest.open(self.root / "manifest/manifest.sqlite")


@pytest.fixture
def no_retry_waits() -> Iterator[None]:
    # A high cap removes real waits while preserving the production attempt limit.
    with stamina.set_testing(testing=True, attempts=100, cap=True):
        yield


@pytest.fixture
def case(tmp_path: Path, monkeypatch: pytest.MonkeyPatch, no_retry_waits: None) -> Case:
    del no_retry_waits
    root = tmp_path / "data"
    monkeypatch.setenv("SVE_DATA_DIR", str(root))
    monkeypatch.setenv("SVE_USER_AGENT", "Mozilla/5.0 synthetic-test")
    monkeypatch.setenv("SVE_INTERVAL", "2.5")
    monkeypatch.setenv("SVE_JITTER", "0")
    for name in ("ROOT", "STORE_ID", "BACKUP_ROOT", "RESTORE_ROOT"):
        monkeypatch.delenv(f"SVE_ARCHIVE_{name}", raising=False)
    clock = FakeClock()
    server = FakeServer(clock)

    def factory(settings: Settings) -> httpx.AsyncClient:
        return httpx.AsyncClient(
            transport=httpx.MockTransport(server),
            headers={"User-Agent": settings.user_agent},
        )

    monkeypatch.setattr(cli, "http_factory", factory)
    monkeypatch.setattr(
        cli, "Throttle", partial(Throttle, clock=clock, sleep=clock.sleep)
    )
    return Case(root, tmp_path / "urls.json", server)


@pytest.mark.parametrize(
    "url",
    [
        "http://shadowverse-evolve.com/errata/x/",
        "https://example.invalid/errata/x/",
        f"{HOST}/rules/",
        f"{HOST}/errata/",
        f"{HOST}/errata/x/extra/",
        f"{HOST}/errata/x/?a=1",
        f"{HOST}/errata/x/#part",
        f"{HOST}/errata/x/?",
        f"{HOST}/errata/x/#",
        "https://user@shadowverse-evolve.com/errata/x/",
        "https://shadowverse-evolve.com:443/errata/x/",
        f"{HOST}/errata/../",
        f"{HOST}/errata/%2e%2e/",
        f"{HOST}/errata/%2f/",
        f"{HOST}/errata/%5c/",
        f"{HOST}/errata/%00/",
        f"{HOST}/errata/%20/",
        f"{HOST}/errata/x/\n",
        f"{HOST}/errata/x\t/",
    ],
)
def test_entire_selection_rejected_before_network_or_manifest(
    case: Case, url: str
) -> None:
    result = case.invoke((URL, url))
    assert result.exit_code == 1, result.output
    assert "stopped:" in result.output
    assert case.server.calls == []
    assert not case.root.exists()


@pytest.mark.parametrize("section", ["rules", "news"])
@pytest.mark.parametrize("host", [HOST, EN_HOST])
def test_other_official_sections_with_a_single_slug_are_rejected(
    case: Case, section: str, host: str
) -> None:
    selection = (URL, f"{host}/{section}/x/")
    with pytest.raises(
        ErrataInputError, match=r"^selection contains a non-official-errata URL$"
    ):
        validate_urls(selection)
    result = case.invoke(selection)
    assert result.exit_code == 1
    assert case.server.calls == []
    assert not case.root.exists()


@pytest.mark.parametrize(
    ("value", "message"),
    [
        ([], "errata URL selection is empty"),
        ({}, "errata URL file must be a JSON array of strings"),
        ("URL", "errata URL file must be a JSON array of strings"),
        ([1], "errata URL file must be a JSON array of strings"),
        ([URL, None], "errata URL file must be a JSON array of strings"),
    ],
)
def test_invalid_selection_file(case: Case, value: object, message: str) -> None:
    case.urls_file.write_text(json.dumps(value))
    with pytest.raises(ErrataInputError, match=f"^{re.escape(message)}$"):
        load_urls(case.urls_file)


def test_dedup_preserves_percent_encoding_and_trailing_slash_distinction(
    case: Case,
) -> None:
    encoded = f"{HOST}/errata/%e3%82%a2/"
    assert validate_urls(
        [encoded, encoded.replace("%e3", "%E3").replace("%a2", "%A2")]
    ) == (f"{HOST}/errata/%E3%82%A2/",)
    result = case.invoke((URL, URL, URL.rstrip("/")))
    assert result.exit_code == 0, result.output
    assert [str(request.url) for _, request in case.server.calls] == [
        URL,
        URL.rstrip("/"),
    ]


@pytest.mark.parametrize("region", [Region.JP, Region.EN])
def test_detail_template_requires_the_full_announcement_structure(
    region: Region,
) -> None:
    url = URL if region is Region.JP else EN_URL
    _validate_body(Response(1, url, 200, "text/html", None, None, notice_body(region)))


@pytest.mark.parametrize("region", [Region.JP, Region.EN])
@pytest.mark.parametrize(
    "damage",
    [
        "outer-chain",
        "layout-heading",
        "notice-title",
        "contents",
        "inner",
        "index",
        "404",
        "home",
        "misplaced-detail",
        "empty-notice",
        "empty-contents",
        "other-region-template",
    ],
)
def test_shared_layout_and_generic_containers_cannot_pass_as_a_notice(
    case: Case, region: Region, damage: str
) -> None:
    url = URL if region is Region.JP else EN_URL
    body = notice_body(region)
    prefix = b"eratta" if region is Region.JP else b"errata"
    replacements = {
        "outer-chain": (b"st-Container_Inner", b"st-Other"),
        "layout-heading": (b"sw-Lower_Heading", b"sw-Other"),
        "notice-title": (b'class="ttl"', b'class="other"'),
        "contents": (b"contents sw-Txtarea", b"contents"),
        "inner": (prefix + b"-Detail_Inner", prefix + b"-Other_Inner"),
        "index": (prefix + b"-Detail", prefix + b"-List"),
        "404": (prefix + b"-Detail", b"err_Txt"),
        "home": (b"sw-Lower", b"sw-Home"),
        "empty-notice": (b"Synthetic correction", b" \n "),
        "empty-contents": (b"Synthetic body", b" "),
        "other-region-template": (
            prefix,
            b"errata" if region is Region.JP else b"eratta",
        ),
    }
    if damage == "misplaced-detail":
        body = body.replace(b'class="sw-Lower_Container"', b'class="other"')
        body = body.replace(
            b"</body>", b'<div class="sw-Lower_Container">Unrelated</div></body>'
        )
    else:
        old, new = replacements[damage]
        body = body.replace(old, new)
    if damage == "empty-contents":
        body = re.sub(rb"<a .*?</a>", b"", body)
    # A generic container must not rescue an index/error or a malformed detail.
    body = body.replace(
        b"</body>",
        b'<main>Generic content</main><article>Generic</article><div class="entry-content">Generic</div></body>',
    )
    with pytest.raises(
        ValidationError, match=r"^HTML lacks a title or announcement body container$"
    ):
        _validate_body(Response(1, url, 200, "text/html", None, None, body))
    case.server.responses = [
        httpx.Response(200, content=body, headers={"content-type": "text/html"})
    ]
    result = case.invoke((url,))
    assert result.exit_code == 1
    assert len(case.server.calls) == 1
    assert not (case.root / raw_path(url)).exists()
    assert body.decode() not in result.output
    with case.manifest() as manifest:
        assert manifest.resources.get(url) is None
        assert manifest.requests.outcomes(url) == [Outcome.FAILED]


def test_mixed_selection_keeps_exact_urls_and_region_specific_raw(case: Case) -> None:
    result = case.invoke((URL, EN_URL))
    assert result.exit_code == 0, result.output
    assert [str(r.url) for _, r in case.server.calls] == [URL, EN_URL]
    assert all(b[0] - a[0] >= 2.5 for a, b in pairwise(case.server.calls))
    with case.manifest() as manifest:
        for url, region in ((URL, Region.JP), (EN_URL, Region.EN)):
            resource = manifest.resources.get(url)
            assert resource is not None
            assert resource.region is region
            assert resource.kind is Kind.ERRATA
            assert resource.path.parts[:3] == ("raw", region.value, "errata")
            assert resource.path == raw_path(url)
            assert decompress((case.root / resource.path).read_bytes()) == notice_body(
                region
            )
            assert resource.sha256 == sha256(notice_body(region))
            assert resource.etag == '"synthetic"'
            assert manifest.links.current(url) == []
            assert manifest.requests.outcomes(url) == [Outcome.CHANGED]
    before = {u: (case.root / raw_path(u)).stat() for u in (URL, EN_URL)}
    assert case.invoke((URL, EN_URL)).exit_code == 0
    assert len(case.server.calls) == 2
    assert {u: (case.root / raw_path(u)).stat() for u in (URL, EN_URL)} == before


@pytest.mark.parametrize(
    "url",
    [
        f"{EN_HOST}/errata/",
        f"{EN_HOST}/errata/x/extra/",
        f"{EN_HOST}/errata/x/?",
        f"{EN_HOST}/errata/%2f/",
        f"{EN_HOST}/errata/%2e%2e/",
        "https://en.shadowverse-evolve.com:443/errata/x/",
        "http://en.shadowverse-evolve.com/errata/x/",
        "https://user@en.shadowverse-evolve.com/errata/x/",
        "https://en.shadowverse-evolve.com.invalid/errata/x/",
    ],
)
def test_en_selection_is_not_a_broad_host_or_path_allowlist(
    case: Case, url: str
) -> None:
    result = case.invoke((EN_URL, url))
    assert result.exit_code == 1
    assert case.server.calls == []
    assert not case.root.exists()


def test_en_original_href_with_trailing_junk_is_rejected_without_repair(
    case: Case,
) -> None:
    malformed = EN_URL + '"'
    with pytest.raises(
        ErrataInputError, match=r"^selection contains a non-official-errata URL$"
    ):
        validate_urls((EN_URL, malformed))
    case.urls_file.write_text(json.dumps([EN_URL, malformed]))
    with pytest.raises(
        ErrataInputError, match=r"^selection contains a non-official-errata URL$"
    ):
        load_urls(case.urls_file)
    assert case.invoke((EN_URL, malformed)).exit_code == 1
    assert case.server.calls == []
    assert not case.root.exists()


@pytest.mark.parametrize("region", [Region.JP, Region.EN])
def test_same_slug_and_raw_region_never_imply_cross_region_trust(
    case: Case, region: Region
) -> None:
    url = URL if region is Region.JP else EN_URL
    assert case.invoke((url,)).exit_code == 0
    with case.manifest() as manifest, manifest.transaction():
        resource = manifest.resources.get(url)
        assert resource is not None
        manifest.resources.put(
            replace(resource, region=Region.EN if region is Region.JP else Region.JP)
        )
    assert case.invoke((url,)).exit_code == 1
    assert len(case.server.calls) == 1


def test_success_has_exact_raw_metadata_no_discovery_and_trusted_skip(
    case: Case,
) -> None:
    result = case.invoke((URL, OTHER))
    assert result.exit_code == 0, result.output
    assert BODY.decode() not in result.output
    assert len(case.server.calls) == 2
    assert all(
        request.headers["user-agent"] == "Mozilla/5.0 synthetic-test"
        for _, request in case.server.calls
    )
    assert all(b[0] - a[0] >= 2.5 for a, b in pairwise(case.server.calls))
    with case.manifest() as manifest:
        resource = manifest.resources.get(URL)
        assert resource is not None
        assert resource.kind is Kind.ERRATA
        assert resource.region is Region.JP
        assert resource.path == raw_path(URL)
        assert resource.sha256 == sha256(BODY)
        assert resource.raw_bytes == len(BODY)
        assert resource.etag == '"synthetic"'
        assert resource.last_modified == "Thu, 01 Oct 2026 00:00:00 GMT"
        assert (
            resource.first_fetched_at
            == resource.last_checked_at
            == resource.last_changed_at
        )
        assert resource.archived_at is None
        assert manifest.requests.outcomes(URL) == [Outcome.CHANGED]
        assert manifest.links.current(URL) == []
    path = case.root / raw_path(URL)
    assert decompress(path.read_bytes()) == BODY
    before = path.stat()
    again = case.invoke()
    assert again.exit_code == 0, again.output
    assert "existing-trusted" in again.output
    assert len(case.server.calls) == 2
    assert path.stat() == before
    with case.manifest() as manifest:
        assert manifest.resources.get(URL) == resource
        assert manifest.requests.outcomes(URL) == [Outcome.CHANGED]


@pytest.mark.parametrize(
    "destination",
    [
        f"{HOST}/rules/",
        f"{HOST}/errata/not-selected/",
        "https://example.invalid/outside",
        OTHER,
        URL,
    ],
)
@pytest.mark.parametrize("source", [URL, EN_URL])
def test_all_redirects_stop_without_contacting_target_or_next_source(
    case: Case, destination: str, source: str
) -> None:
    case.server.responses = [httpx.Response(302, headers={"location": destination})]
    result = case.invoke((source, OTHER))
    assert result.exit_code == 1, result.output
    assert "redirect refused" in result.output
    assert len(case.server.calls) == 1
    assert not (case.root / raw_path(source)).exists()
    with case.manifest() as manifest:
        assert list(manifest.resources.all()) == []
        assert manifest.requests.outcomes(source) == [Outcome.REDIRECTED]


@pytest.mark.parametrize(
    "failure",
    [
        httpx.Response(503),
        httpx.ReadTimeout("synthetic timeout"),
        httpx.ConnectError("synthetic connection"),
    ],
)
@pytest.mark.parametrize("region", [Region.JP, Region.EN])
def test_only_three_attempts_then_continue_next_url(
    case: Case, failure: httpx.Response | BaseException, region: Region
) -> None:
    url, other = (URL, OTHER) if region is Region.JP else (EN_URL, EN_OTHER)
    case.server.responses = [failure] * 3 + [ok(region)]
    result = case.invoke((url, other))
    assert result.exit_code == 1, result.output
    assert [str(request.url) for _, request in case.server.calls] == [
        url,
        url,
        url,
        other,
    ]
    assert all(b[0] - a[0] >= 2.5 for a, b in pairwise(case.server.calls))
    with case.manifest() as manifest:
        assert manifest.resources.get(url) is None
        assert manifest.resources.get(other) is not None
        assert manifest.requests.outcomes(url) == [Outcome.FAILED] * 3


def test_success_on_third_attempt(case: Case) -> None:
    case.server.responses = [httpx.Response(500), httpx.Response(500), ok()]
    result = case.invoke()
    assert result.exit_code == 0, result.output
    assert len(case.server.calls) == 3


@pytest.mark.parametrize("source", [URL, EN_URL])
def test_cli_total_budget_stops_an_adapter_before_a_fourth_request(
    case: Case, monkeypatch: pytest.MonkeyPatch, source: str
) -> None:
    async def probe(
        selection: tuple[str, ...],
        client: Client,
        writer: Writer,
        _breaker: CircuitBreaker,
        _report: Callable[[ErrataResult], None],
    ) -> int:
        request = Request(selection[0], allowed=selection.__contains__)
        # Force successful extra requests independently of the per-URL retry limit.
        for _ in range(3):
            response = await client.get(request)
            with writer.manifest.transaction():
                writer.manifest.requests.finish(
                    response.request_id,
                    RequestResult(Outcome.FAILED, validation_error="synthetic probe"),
                )
        with pytest.raises(
            BudgetExhaustedError, match=r"^request budget of 3 reached$"
        ):
            await client.get(request)
        return 0

    monkeypatch.setattr(cli, "fetch_new", probe)
    result = case.invoke((source,))
    assert result.exit_code == 0, result.output
    assert len(case.server.calls) == 3
    assert all(str(request.url) == source for _, request in case.server.calls)
    with case.manifest() as manifest:
        assert manifest.requests.outcomes(source) == [Outcome.FAILED] * 3
        assert manifest.resources.get(source) is None


@pytest.mark.parametrize(
    ("status", "body", "content_type"),
    [
        (404, BODY, "text/html"),
        (304, b"", "text/html"),
        (200, BODY, "image/png"),
        (200, b"\xff", "text/html"),
        (
            200,
            b"<html><title>Error</title><body>Missing notice</body></html>",
            "text/html",
        ),
    ],
    ids=["missing", "unexpected-304", "wrong-media", "invalid-utf8", "no-container"],
)
def test_invalid_response_records_failure_without_saving_body(
    case: Case, status: int, body: bytes, content_type: str
) -> None:
    case.server.responses = [
        httpx.Response(status, content=body, headers={"content-type": content_type})
    ]
    result = case.invoke()
    assert result.exit_code == 1, result.output
    assert len(case.server.calls) == 1
    assert not (case.root / raw_path(URL)).exists()
    assert "Missing notice" not in result.output
    with case.manifest() as manifest:
        assert manifest.resources.get(URL) is None
        assert manifest.requests.outcomes(URL) == [Outcome.FAILED]


@pytest.mark.parametrize(
    "title",
    [b"", b"<title></title>", b"<title> \n\t </title>"],
    ids=["absent", "empty", "whitespace"],
)
def test_notice_requires_a_nonempty_title(case: Case, title: bytes) -> None:
    body = BODY.replace(b"<title>Synthetic notice</title>", title)
    response = Response(1, URL, 200, "text/html", None, None, body)
    with pytest.raises(
        ValidationError, match=r"^HTML lacks a title or announcement body container$"
    ):
        _validate_body(response)
    case.server.responses = [
        httpx.Response(200, content=body, headers={"content-type": "text/html"})
    ]
    result = case.invoke()
    assert result.exit_code == 1
    assert len(case.server.calls) == 1
    assert not (case.root / raw_path(URL)).exists()
    with case.manifest() as manifest:
        assert manifest.resources.get(URL) is None
        assert manifest.requests.outcomes(URL) == [Outcome.FAILED]


@pytest.mark.parametrize("state", ["file", "directory", "symlink", "dangling"])
@pytest.mark.parametrize("source", [URL, EN_URL])
def test_any_preexisting_destination_stops_before_network(
    case: Case, tmp_path: Path, state: str, source: str
) -> None:
    path = case.root / raw_path(source)
    path.parent.mkdir(parents=True)
    original = b"untracked raw must survive"
    outside = tmp_path / "outside"
    outside.write_bytes(original)
    if state == "file":
        path.write_bytes(original)
    elif state == "directory":
        path.mkdir()
    else:
        path.symlink_to(outside if state == "symlink" else tmp_path / "missing")
    result = case.invoke((OTHER, source))
    assert result.exit_code == 1, result.output
    assert case.server.calls == []
    assert outside.read_bytes() == original
    assert path.is_symlink() if state in {"symlink", "dangling"} else path.exists()
    if state == "file":
        assert path.read_bytes() == original


@pytest.mark.parametrize("source", [URL, EN_URL])
def test_symlink_parent_inside_data_root_is_rejected_before_requests(
    case: Case,
    source: str,
) -> None:
    redirect = case.root / "alternate-errata"
    redirect.mkdir(parents=True)
    sentinel = redirect / "sentinel"
    sentinel.write_bytes(b"other task must survive")
    parent = case.root / raw_path(source).parent
    parent.parent.mkdir(parents=True)
    parent.symlink_to(redirect, target_is_directory=True)
    with case.manifest() as manifest:
        writer = Writer(case.root, manifest, create_only=True)
        message = re.escape(
            f"new-only destination traverses a symlink: {raw_path(source)}"
        )
        with pytest.raises(UnsafePathError, match=f"^{message}$"):
            writer.check_new(URL, raw_path(source))
    result = case.invoke((source,))
    assert result.exit_code == 1
    assert case.server.calls == []
    assert list(redirect.iterdir()) == [sentinel]
    assert sentinel.read_bytes() == b"other task must survive"
    assert parent.is_symlink()


@pytest.mark.parametrize(
    "damage", ["delete", "corrupt", "archived", "wrong-kind", "wrong-region"]
)
def test_recorded_untrusted_source_never_repaired(case: Case, damage: str) -> None:
    assert case.invoke().exit_code == 0
    path = case.root / raw_path(URL)
    if damage == "delete":
        path.unlink()
    elif damage == "corrupt":
        path.write_bytes(b"damaged")
    else:
        with case.manifest() as manifest, manifest.transaction():
            resource = manifest.resources.get(URL)
            assert resource is not None
            if damage == "archived":
                resource = replace(resource, archived_at=resource.first_fetched_at)
            elif damage == "wrong-kind":
                resource = replace(resource, kind=Kind.CARD)
            else:
                resource = replace(resource, region=Region.EN)
            manifest.resources.put(resource)
    before = path.read_bytes() if path.exists() else None
    result = case.invoke((OTHER, URL))
    assert result.exit_code == 1, result.output
    assert len(case.server.calls) == 1
    assert (path.read_bytes() if path.exists() else None) == before


def test_file_appearing_during_publication_cannot_be_overwritten(
    case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    link = os.link
    target = case.root / raw_path(URL)

    def race(source: Path, dest: Path, *, follow_symlinks: bool) -> None:
        target.write_bytes(b"raced-in existing raw")
        link(source, dest, follow_symlinks=follow_symlinks)

    monkeypatch.setattr(os, "link", race)
    result = case.invoke()
    assert result.exit_code == 1, result.output
    assert target.read_bytes() == b"raced-in existing raw"
    assert list(case.root.rglob("*.tmp-*")) == []
    with case.manifest() as manifest:
        assert manifest.resources.get(URL) is None


@pytest.mark.parametrize(
    "error", [KeyboardInterrupt(), OSError(errno.ENOSPC, "synthetic disk full")]
)
@pytest.mark.parametrize("source", [URL, EN_URL])
def test_interruption_before_publish_leaves_no_partial_raw_or_own_temp(
    case: Case, monkeypatch: pytest.MonkeyPatch, error: BaseException, source: str
) -> None:
    def fail(_fd: int) -> None:
        raise error

    with monkeypatch.context() as failure_patch:
        failure_patch.setattr(os, "fsync", fail)
        result = case.invoke((source,))
    assert result.exit_code != 0
    assert not (case.root / raw_path(source)).exists()
    assert list(case.root.rglob("*.tmp-*")) == []
    with case.manifest() as manifest:
        assert manifest.resources.get(source) is None
        assert manifest.requests.outcomes(source) == [Outcome.STARTED]
    again = case.invoke((source,))
    assert again.exit_code == 1
    assert len(case.server.calls) == 1


@pytest.mark.parametrize("source", [URL, EN_URL])
def test_interruption_after_publish_preserves_complete_unregistered_raw(
    case: Case, monkeypatch: pytest.MonkeyPatch, source: str
) -> None:
    def fail_record(*_args: object) -> None:
        raise KeyboardInterrupt

    monkeypatch.setattr(Writer, "_record", fail_record)
    result = case.invoke((source,))
    assert result.exit_code != 0
    assert decompress((case.root / raw_path(source)).read_bytes()) == notice_body(
        url_region(source)
    )
    assert list(case.root.rglob("*.tmp-*")) == []
    with case.manifest() as manifest:
        assert manifest.resources.get(source) is None


def test_unrelated_interrupted_requests_are_not_recovered(case: Case) -> None:
    with case.manifest() as manifest:
        manifest.requests.start(
            RequestStart("other-run", "other-fetch", 1, 0, OTHER, OTHER, None)
        )
    result = case.invoke()
    assert result.exit_code == 1
    assert case.server.calls == []
    with case.manifest() as manifest:
        assert manifest.requests.outcomes(OTHER) == [Outcome.STARTED]


def test_unrelated_temporary_file_is_not_removed(case: Case) -> None:
    path = case.root / "raw/jp/card/unrelated.html.zst.tmp-abandoned"
    path.parent.mkdir(parents=True)
    path.write_bytes(b"unfinished other task")
    result = case.invoke()
    assert result.exit_code == 1
    assert case.server.calls == []
    assert path.read_bytes() == b"unfinished other task"


def test_lock_conflict_makes_no_requests(case: Case) -> None:
    with ExclusiveLock(case.root / "manifest/.lock"):
        result = case.invoke()
    assert result.exit_code == 1
    assert case.server.calls == []
    assert not (case.root / "manifest/manifest.sqlite").exists()


def test_dry_run_does_not_create_manifest_or_raw(case: Case) -> None:
    result = case.invoke(dry_run=True)
    assert result.exit_code == 0, result.output
    assert URL in result.output
    assert case.server.calls == []
    assert not (case.root / "manifest/manifest.sqlite").exists()
    assert not (case.root / "raw").exists()


def test_create_only_writer_cannot_update_existing_resource(case: Case) -> None:
    assert case.invoke().exit_code == 0
    with case.manifest() as manifest:
        writer = Writer(case.root, manifest, create_only=True)
        message = re.escape(f"new-only source already recorded: {URL}")
        with pytest.raises(PathConflictError, match=f"^{message}$"):
            writer.check_new(URL, raw_path(URL))


@pytest.mark.parametrize("source", [URL, EN_URL])
async def test_fetch_new_rejects_an_ordinary_writer_before_requests(
    case: Case, source: str
) -> None:
    reports: list[ErrataResult] = []
    with case.manifest() as manifest:
        async with httpx.AsyncClient(
            transport=httpx.MockTransport(case.server)
        ) as http:
            client = Client(
                http,
                Throttle(
                    2.5, 0, clock=case.server.clock, sleep=case.server.clock.sleep
                ),
                manifest,
                run_id="synthetic-create-only-probe",
            )
            with pytest.raises(
                StopCrawlError, match=r"^errata fetch requires a create-only writer$"
            ):
                await fetch_new(
                    (source,),
                    client,
                    Writer(case.root, manifest),
                    CircuitBreaker(5),
                    reports.append,
                )
            assert client.requests_sent == 0
            assert manifest.requests.outcomes(source) == []
            assert manifest.resources.get(source) is None
    assert case.server.calls == []
    assert reports == []
    assert not (case.root / "raw").exists()


def configure_archive(case: Case, monkeypatch: pytest.MonkeyPatch) -> Path:
    root = case.root.parent / "archive"
    for name, value in (
        ("ROOT", str(root)),
        ("STORE_ID", "synthetic-store"),
        ("BACKUP_ROOT", str(case.root.parent / "backup")),
        ("RESTORE_ROOT", str(case.root.parent / "restore")),
    ):
        monkeypatch.setenv(f"SVE_ARCHIVE_{name}", value)
    return root


def test_partial_archive_configuration_stops_before_opening_manifest(
    case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("SVE_ARCHIVE_STORE_ID", "synthetic-store")
    result = case.invoke()
    assert result.exit_code == 1
    assert "incomplete archive configuration" in result.output
    assert case.server.calls == []
    assert not case.root.exists()


def test_complete_config_does_not_initialize_archive_or_run_general_recovery(
    case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    root = configure_archive(case, monkeypatch)

    def unexpected(*_args: object, **_kwargs: object) -> None:
        pytest.fail("new-only errata fetch invoked general recovery/archive writer")

    monkeypatch.setattr(cli, "_recover", unexpected)
    monkeypatch.setattr(cli, "_refresh_writer", unexpected)
    result = case.invoke()
    assert result.exit_code == 0, result.output
    assert not root.exists()
    assert not (case.root.parent / "backup").exists()
    assert not (case.root.parent / "restore").exists()


@pytest.mark.parametrize("directory", ["replacements", "observations", "staging"])
def test_pending_archive_work_is_preserved_without_recovery(
    case: Case, monkeypatch: pytest.MonkeyPatch, directory: str
) -> None:
    root = configure_archive(case, monkeypatch)
    path = root / directory / "synthetic.json"
    path.parent.mkdir(parents=True)
    path.write_bytes(b"unfinished other archive task")
    result = case.invoke()
    assert result.exit_code == 1, result.output
    assert case.server.calls == []
    assert path.read_bytes() == b"unfinished other archive task"
    assert list(root.rglob("*")) == [path.parent, path]


@pytest.mark.parametrize(
    ("directory", "markers"),
    [
        ("replacements", "replacement-completions"),
        ("observations", "observation-seals"),
    ],
)
def test_completed_other_archive_work_is_untouched(
    case: Case, monkeypatch: pytest.MonkeyPatch, directory: str, markers: str
) -> None:
    root = configure_archive(case, monkeypatch)
    for name in (directory, markers):
        path = root / name / "synthetic.json"
        path.parent.mkdir(parents=True)
        path.write_bytes(b"completed other archive task")
    before = {
        path.relative_to(root): path.read_bytes()
        for path in root.rglob("*")
        if path.is_file()
    }
    result = case.invoke()
    assert result.exit_code == 0, result.output
    assert {
        path.relative_to(root): path.read_bytes()
        for path in root.rglob("*")
        if path.is_file()
    } == before


@pytest.mark.parametrize(
    "responses",
    [
        [httpx.Response(403)],
        [httpx.Response(429, headers={"retry-after": "0"}), httpx.Response(429)],
    ],
)
def test_access_denied_or_repeated_rate_limit_stops_entire_run(
    case: Case, responses: list[httpx.Response]
) -> None:
    case.server.responses.extend(responses)
    result = case.invoke((URL, OTHER))
    assert result.exit_code == 1, result.output
    assert len(case.server.calls) == len(responses)
    assert all(str(request.url) == URL for _, request in case.server.calls)


def test_circuit_breaker_stops_after_configured_failures(
    case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("SVE_BREAKER_THRESHOLD", "1")
    case.server.responses = [httpx.Response(404)]
    result = case.invoke((URL, OTHER))
    assert result.exit_code == 1
    assert len(case.server.calls) == 1


def test_dry_run_existing_source_keeps_resource_metadata(case: Case) -> None:
    assert case.invoke().exit_code == 0
    with case.manifest() as manifest:
        before = manifest.resources.get(URL)
    result = case.invoke(dry_run=True)
    assert result.exit_code == 0, result.output
    assert "existing-trusted" in result.output
    assert len(case.server.calls) == 1
    with case.manifest() as manifest:
        assert manifest.resources.get(URL) == before


def test_created_temp_name_collision_never_deletes_another_file(
    case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = case.root / raw_path(URL)
    temp = path.with_name(path.name + ".tmp-collision")
    original_check = Writer.check_new

    def add_temp_after_preflight(writer: Writer, url: str, raw: object) -> None:
        assert raw == raw_path(url)
        original_check(writer, url, raw_path(url))
        temp.parent.mkdir(parents=True, exist_ok=True)
        temp.write_bytes(b"unrelated existing temp")

    monkeypatch.setattr(Writer, "check_new", add_temp_after_preflight)
    monkeypatch.setattr(
        "sve_carddb.fetch.writer.secrets.token_hex", lambda _count: "collision"
    )
    result = case.invoke()
    assert result.exit_code == 1, result.output
    assert not path.exists()
    assert temp.read_bytes() == b"unrelated existing temp"


class ErrataHandler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        if self.headers.get("User-Agent") != "Mozilla/5.0 synthetic-test":
            self.send_response(400)
        elif self.path == "/errata/redirect/":
            self.send_response(302)
            self.send_header("Location", f"{HOST}/rules/")
        elif self.path == "/errata/retry/":
            self.send_response(503)
        else:
            self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("ETag", '"synthetic"')
        self.send_header("Content-Length", str(len(BODY)))
        self.end_headers()
        self.wfile.write(BODY)

    @override
    def log_message(self, _format: str, *_args: object) -> None:
        pass


@pytest.fixture(scope="module")
def localhost_errata_server() -> Iterator[str]:
    server = ThreadingHTTPServer(("127.0.0.1", 0), ErrataHandler)
    thread = Thread(target=server.serve_forever, kwargs={"poll_interval": 0.01})
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=1)
        assert not thread.is_alive()


class LocalErrataTransport(httpx.AsyncBaseTransport):
    def __init__(self, base: str, calls: list[httpx.Request]) -> None:
        self.base = httpx.URL(base)
        self.calls = calls
        self.transport = httpx.AsyncHTTPTransport()

    @override
    async def handle_async_request(self, request: httpx.Request) -> httpx.Response:
        self.calls.append(request)
        # Keep production URLs in the manifest, but send every byte only to loopback.
        local = request.url.copy_with(
            scheme=self.base.scheme, host=self.base.host, port=self.base.port
        )
        return await self.transport.handle_async_request(
            httpx.Request(request.method, local, headers=request.headers)
        )

    @override
    async def aclose(self) -> None:
        await self.transport.aclose()


@pytest.mark.parametrize(
    ("slug", "expected"),
    [
        ("success", (0, 1, Outcome.CHANGED)),
        ("redirect", (1, 1, Outcome.REDIRECTED)),
        ("retry", (1, 3, Outcome.FAILED)),
    ],
)
def test_localhost_http_server_obeys_publication_redirect_and_retry_rules(
    case: Case,
    monkeypatch: pytest.MonkeyPatch,
    localhost_errata_server: str,
    slug: str,
    expected: tuple[int, int, Outcome],
) -> None:
    exit_code, requests, outcome = expected
    calls: list[httpx.Request] = []

    def factory(settings: Settings) -> httpx.AsyncClient:
        return httpx.AsyncClient(
            transport=LocalErrataTransport(localhost_errata_server, calls),
            headers={"User-Agent": settings.user_agent},
            timeout=1,
        )

    monkeypatch.setattr(cli, "http_factory", factory)
    url = f"{HOST}/errata/{slug}/"
    result = case.invoke((url,))
    assert result.exit_code == exit_code, result.output
    assert len(calls) == requests
    assert all(str(request.url) == url for request in calls)
    assert all(
        request.headers["user-agent"] == "Mozilla/5.0 synthetic-test"
        for request in calls
    )
    with case.manifest() as manifest:
        assert manifest.requests.outcomes(url) == [outcome] * requests
        resource = manifest.resources.get(url)
    if exit_code == 0:
        assert resource is not None
        assert resource.sha256 == sha256(BODY)
        assert decompress((case.root / raw_path(url)).read_bytes()) == BODY
    else:
        assert resource is None
        assert not (case.root / raw_path(url)).exists()
