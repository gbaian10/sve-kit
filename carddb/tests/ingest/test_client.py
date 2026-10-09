from datetime import UTC, datetime
from itertools import pairwise
from typing import TYPE_CHECKING

import httpx
import pytest

from sve_carddb.ingest.archive.manifest import Manifest, Outcome
from sve_carddb.ingest.http.client import (
    BudgetExhaustedError,
    Client,
    ClientPolicy,
    FetchError,
    Request,
    StopCrawlError,
    retry_after_seconds,
)
from sve_carddb.ingest.http.throttle import Throttle

if TYPE_CHECKING:
    from collections.abc import Callable, Iterator

    from ..conftest import FakeClock

HOST = "https://shadowverse-evolve.com"
URL = f"{HOST}/cardlist/?cardno=BP01-001"
INTERVAL = 2.5
NO_WAIT = ClientPolicy(wait_initial=0.0, wait_max=0.0, wait_jitter=0.0)


def same_host(url: str) -> bool:
    return url.startswith(f"{HOST}/")


class Server:
    """Replays scripted responses and records when each request arrived."""

    def __init__(
        self, clock: FakeClock, script: Iterator[httpx.Response | Exception]
    ) -> None:
        self.clock = clock
        self.script = script
        self.calls: list[tuple[float, httpx.Request]] = []

    def __call__(self, request: httpx.Request) -> httpx.Response:
        self.calls.append((self.clock.now, request))
        step = next(self.script)
        if isinstance(step, Exception):
            raise step
        return step


def make_client(
    manifest: Manifest, clock: FakeClock, *steps: httpx.Response | Exception
) -> tuple[Client, Server]:
    server = Server(clock, iter(steps))
    http = httpx.AsyncClient(transport=httpx.MockTransport(server))
    throttle = Throttle(INTERVAL, 0.0, clock=clock, sleep=clock.sleep)
    client = Client(http, throttle, manifest, run_id="run", policy=NO_WAIT, clock=clock)
    return client, server


def ok(body: bytes = b"<html>card</html>", **headers: str) -> httpx.Response:
    return httpx.Response(
        200, content=body, headers={"content-type": "text/html", **headers}
    )


def status(code: int, **headers: str) -> httpx.Response:
    return httpx.Response(code, headers=headers)


def request(
    allowed: Callable[[str], bool] = same_host, etag: str | None = None
) -> Request:
    return Request(url=URL, allowed=allowed, if_none_match=etag)


def gaps(server: Server) -> list[float]:
    times = [at for at, _ in server.calls]
    return [later - earlier for earlier, later in pairwise(times)]


async def test_success_returns_body_and_leaves_the_log_open(
    manifest: Manifest, clock: FakeClock
) -> None:
    client, _ = make_client(manifest, clock, ok(etag='"abc"'))
    response = await client.get(request())
    assert response.status == 200
    assert response.body == b"<html>card</html>"
    assert response.etag == '"abc"'
    assert manifest.requests.outcomes(URL) == [Outcome.STARTED]


async def test_retries_503_and_every_attempt_is_throttled(
    manifest: Manifest, clock: FakeClock
) -> None:
    client, server = make_client(manifest, clock, status(503), status(503), ok())
    response = await client.get(request())
    assert response.status == 200
    assert all(gap >= INTERVAL for gap in gaps(server))
    assert manifest.requests.outcomes(URL) == [
        Outcome.FAILED,
        Outcome.FAILED,
        Outcome.STARTED,
    ]


async def test_retries_timeouts(manifest: Manifest, clock: FakeClock) -> None:
    client, server = make_client(manifest, clock, httpx.ReadTimeout("slow"), ok())
    await client.get(request())
    assert len(server.calls) == 2


async def test_gives_up_after_four_attempts(
    manifest: Manifest, clock: FakeClock
) -> None:
    client, server = make_client(manifest, clock, *[status(500)] * 4)
    with pytest.raises(FetchError, match="gave up"):
        await client.get(request())
    assert len(server.calls) == 4


@pytest.mark.parametrize("code", [304, 404])
async def test_304_and_404_are_returned_without_retry(
    manifest: Manifest, clock: FakeClock, code: int
) -> None:
    client, server = make_client(manifest, clock, status(code))
    assert (await client.get(request())).status == code
    assert len(server.calls) == 1


async def test_unexpected_status_fails_without_retry(
    manifest: Manifest, clock: FakeClock
) -> None:
    client, server = make_client(manifest, clock, status(400))
    with pytest.raises(FetchError, match="unexpected HTTP 400"):
        await client.get(request())
    assert len(server.calls) == 1


async def test_403_stops_the_run_at_once(manifest: Manifest, clock: FakeClock) -> None:
    client, server = make_client(manifest, clock, status(403), ok())
    with pytest.raises(StopCrawlError, match="403"):
        await client.get(request())
    assert len(server.calls) == 1


async def test_429_waits_for_retry_after(manifest: Manifest, clock: FakeClock) -> None:
    client, server = make_client(
        manifest, clock, status(429, **{"retry-after": "120"}), ok()
    )
    await client.get(request())
    assert gaps(server) == [pytest.approx(120.0)]


async def test_429_without_retry_after_uses_the_default(
    manifest: Manifest, clock: FakeClock
) -> None:
    client, server = make_client(manifest, clock, status(429), ok())
    await client.get(request())
    assert gaps(server) == [pytest.approx(60.0)]


async def test_second_429_stops_the_run(manifest: Manifest, clock: FakeClock) -> None:
    client, _ = make_client(manifest, clock, status(429), ok(), status(429))
    await client.get(request())
    with pytest.raises(StopCrawlError, match="second 429"):
        await client.get(request())


async def test_follows_allowed_redirects_hop_by_hop(
    manifest: Manifest, clock: FakeClock
) -> None:
    client, server = make_client(
        manifest,
        clock,
        status(301, location="/cardlist/?cardno=BP01-001&moved=1"),
        ok(),
    )
    response = await client.get(request())
    assert response.url == f"{HOST}/cardlist/?cardno=BP01-001&moved=1"
    assert all(gap >= INTERVAL for gap in gaps(server))
    assert manifest.requests.outcomes(URL) == [Outcome.REDIRECTED, Outcome.STARTED]


async def test_disallowed_redirect_is_never_sent(
    manifest: Manifest, clock: FakeClock
) -> None:
    client, server = make_client(
        manifest, clock, status(302, location="https://evil.example/")
    )
    with pytest.raises(FetchError, match="disallowed"):
        await client.get(request())
    assert [str(r.url) for _, r in server.calls] == [URL]


async def test_redirect_loop_fails(manifest: Manifest, clock: FakeClock) -> None:
    client, _ = make_client(
        manifest,
        clock,
        status(302, location=f"{HOST}/a"),
        status(302, location=URL),
    )
    with pytest.raises(FetchError, match="loop"):
        await client.get(request())


async def test_too_many_redirects_fail(manifest: Manifest, clock: FakeClock) -> None:
    hops = [status(302, location=f"{HOST}/hop{i}") for i in range(6)]
    client, _ = make_client(manifest, clock, *hops)
    with pytest.raises(FetchError, match="more than 5 redirects"):
        await client.get(request())


async def test_if_none_match_only_on_the_first_hop(
    manifest: Manifest, clock: FakeClock
) -> None:
    client, server = make_client(
        manifest, clock, status(302, location=f"{HOST}/moved"), status(304)
    )
    await client.get(request(etag='"abc"'))
    sent = [r.headers.get("if-none-match") for _, r in server.calls]
    assert sent == ['"abc"', None]


async def test_request_budget_counts_retries(
    manifest: Manifest, clock: FakeClock
) -> None:
    server = Server(clock, iter([status(503), status(503), ok()]))
    http = httpx.AsyncClient(transport=httpx.MockTransport(server))
    throttle = Throttle(INTERVAL, 0.0, clock=clock, sleep=clock.sleep)
    policy = ClientPolicy(
        wait_initial=0.0, wait_max=0.0, wait_jitter=0.0, max_requests=2
    )
    client = Client(http, throttle, manifest, run_id="run", policy=policy, clock=clock)
    with pytest.raises(BudgetExhaustedError, match="budget of 2"):
        await client.get(request())
    assert client.requests_sent == 2
    assert len(server.calls) == 2


NOW = datetime(2026, 9, 24, 12, 0, tzinfo=UTC)


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        pytest.param("120", 120.0, id="seconds"),
        pytest.param("Thu, 24 Sep 2026 12:05:00 GMT", 300.0, id="future date"),
        pytest.param("Thu, 24 Sep 2026 11:00:00 GMT", 0.0, id="past date"),
        pytest.param("soon", 60.0, id="invalid"),
        pytest.param(None, 60.0, id="missing"),
    ],
)
def test_retry_after_seconds(value: str | None, expected: float) -> None:
    assert retry_after_seconds(value, NOW, default=60.0) == pytest.approx(expected)
