"""The HTTP client every crawl request goes through.

- Every actual request, including each retry and each redirect hop, waits on the
  throttle and gets its own fetch_log row.
- Redirects are followed by hand: the destination is checked before the request
  is sent, so a disallowed URL is never contacted.
- Only timeouts, connection errors and 500/502/503/504 are retried.
- 403 stops the run at once; a second 429 in one run stops it too.
"""

import time
import uuid
from dataclasses import dataclass
from datetime import UTC, datetime
from email.utils import parsedate_to_datetime
from typing import TYPE_CHECKING
from urllib.parse import urljoin

import httpx
import stamina

from sve_carddb.manifest import Outcome, RequestResult, RequestStart
from sve_carddb.urls import canonicalize

if TYPE_CHECKING:
    from collections.abc import Callable

    from sve_carddb.fetch.throttle import Throttle
    from sve_carddb.manifest import Manifest

_RETRYABLE_STATUS = frozenset({500, 502, 503, 504})
_RETURNED_STATUS = frozenset({200, 304, 404})
_REDIRECT_STATUS = frozenset({301, 302, 303, 307, 308})
_FORBIDDEN = 403
_TOO_MANY_REQUESTS = 429


class StopCrawlError(RuntimeError):
    """A condition that must stop the whole run, such as 403 or a repeated 429."""


class BudgetExhaustedError(RuntimeError):
    """The run reached its `max_requests` budget; stop cleanly."""


class FetchError(RuntimeError):
    """This URL could not be fetched; the run may go on (the circuit breaker counts it)."""


class _RetryableError(Exception):
    pass


@dataclass(frozen=True, slots=True)
class Request:
    """What to fetch. `allowed` is checked against every hop before it is sent."""

    url: str
    allowed: Callable[[str], bool]
    if_none_match: str | None = None
    headers: tuple[tuple[str, str], ...] = ()
    """Sent on every hop, for APIs that pick the language by header."""


@dataclass(frozen=True, slots=True)
class Response:
    """A 200, 304 or 404. Its fetch_log row `request_id` is still open for the caller to finish."""

    request_id: int
    url: str
    status: int
    content_type: str | None
    etag: str | None
    last_modified: str | None
    body: bytes


@dataclass(frozen=True, slots=True)
class ClientPolicy:
    """Retry and redirect limits. `attempts` counts the first try."""

    attempts: int = 4
    timeout: float = 300.0
    wait_initial: float = 5.0
    wait_max: float = 60.0
    wait_jitter: float = 1.0
    max_hops: int = 5
    default_retry_after: float = 60.0
    """Seconds to wait after a 429 whose Retry-After is missing or invalid."""
    max_requests: int | None = None
    """Hard cap on actual HTTP requests in this run, retries and redirects included."""


def retry_after_seconds(value: str | None, now: datetime, default: float) -> float:
    """Parse `Retry-After` as seconds or an HTTP date; fall back to `default`."""
    if value is None:
        return default
    text = value.strip()
    if text.isdigit():
        return float(text)
    try:
        when = parsedate_to_datetime(text)
    except TypeError, ValueError:
        return default
    if when.tzinfo is None:
        when = when.replace(tzinfo=UTC)
    return max(0.0, (when - now).total_seconds())


class Client:
    """Fetch URLs politely and record every request in the manifest."""

    def __init__(
        self,
        http: httpx.AsyncClient,
        throttle: Throttle,
        manifest: Manifest,
        *,
        run_id: str,
        policy: ClientPolicy | None = None,
        clock: Callable[[], float] = time.monotonic,
    ) -> None:
        """Wrap `http`; requests are spaced by `throttle` and logged in `manifest`."""
        self._http = http
        self._throttle = throttle
        self._manifest = manifest
        self._run_id = run_id
        self._policy = policy or ClientPolicy()
        self._clock = clock
        self._not_before: float | None = None
        self._seen_429 = 0
        self._requests_sent = 0

    @property
    def requests_sent(self) -> int:
        """Actual HTTP requests sent so far in this run."""
        return self._requests_sent

    async def get(self, request: Request) -> Response:
        """Fetch `request`; raise `FetchError` or `StopCrawlError` on failure."""
        logical_fetch_id = uuid.uuid4().hex
        policy = self._policy
        try:
            async for attempt in stamina.retry_context(
                on=_RetryableError,
                attempts=policy.attempts,
                timeout=policy.timeout,
                wait_initial=policy.wait_initial,
                wait_max=policy.wait_max,
                wait_jitter=policy.wait_jitter,
            ):
                with attempt:
                    return await self._attempt(request, logical_fetch_id, attempt.num)
        except _RetryableError as exc:
            msg = f"{request.url}: gave up after retries ({exc})"
            raise FetchError(msg) from exc
        msg = f"{request.url}: no attempt was made"
        raise FetchError(msg)

    async def _attempt(
        self, request: Request, logical_fetch_id: str, attempt: int
    ) -> Response:
        url = request.url
        visited = {url}
        for hop in range(self._policy.max_hops + 1):
            if not request.allowed(url):
                msg = f"{request.url}: redirect to disallowed {url}"
                raise FetchError(msg)
            budget = self._policy.max_requests
            if budget is not None and self._requests_sent >= budget:
                msg = f"request budget of {budget} reached"
                raise BudgetExhaustedError(msg)
            await self._throttle.wait(not_before=self._not_before)
            self._requests_sent += 1
            if_none_match = request.if_none_match if hop == 0 else None
            request_id = self._manifest.requests.start(
                RequestStart(
                    run_id=self._run_id,
                    logical_fetch_id=logical_fetch_id,
                    attempt=attempt,
                    hop=hop,
                    url=request.url,
                    requested_url=url,
                    sent_if_none_match=if_none_match,
                )
            )
            response = await self._send(url, request.headers, if_none_match, request_id)
            status = response.status_code
            if status in _RETURNED_STATUS:
                return Response(
                    request_id=request_id,
                    url=url,
                    status=status,
                    content_type=response.headers.get("content-type"),
                    etag=response.headers.get("etag"),
                    last_modified=response.headers.get("last-modified"),
                    body=response.content,
                )
            location = response.headers.get("location")
            if status in _REDIRECT_STATUS and location:
                self._finish(request_id, Outcome.REDIRECTED, status=status)
                url = canonicalize(urljoin(url, location))
                if url in visited:
                    msg = f"{request.url}: redirect loop at {url}"
                    raise FetchError(msg)
                visited.add(url)
                continue
            self._handle_error_status(request_id, status, response)
        msg = f"{request.url}: more than {self._policy.max_hops} redirects"
        raise FetchError(msg)

    async def _send(
        self,
        url: str,
        extra: tuple[tuple[str, str], ...],
        if_none_match: str | None,
        request_id: int,
    ) -> httpx.Response:
        headers = dict(extra)
        if if_none_match:
            headers["If-None-Match"] = if_none_match
        try:
            return await self._http.get(url, headers=headers, follow_redirects=False)
        except httpx.TimeoutException as exc:
            self._finish(request_id, Outcome.FAILED, error_class="timeout")
            raise _RetryableError(str(exc)) from exc
        except httpx.TransportError as exc:
            self._finish(request_id, Outcome.FAILED, error_class="connect")
            raise _RetryableError(str(exc)) from exc

    def _handle_error_status(
        self, request_id: int, status: int, response: httpx.Response
    ) -> None:
        self._finish(
            request_id, Outcome.FAILED, status=status, error_class="http_status"
        )
        if status == _FORBIDDEN:
            msg = f"403 Forbidden for {response.url}; stopping to investigate"
            raise StopCrawlError(msg)
        if status == _TOO_MANY_REQUESTS:
            self._seen_429 += 1
            if self._seen_429 > 1:
                msg = "second 429 in this run; stopping"
                raise StopCrawlError(msg)
            delay = retry_after_seconds(
                response.headers.get("retry-after"),
                datetime.now(UTC),
                self._policy.default_retry_after,
            )
            self._not_before = self._clock() + delay
            msg = f"429, waiting {delay:.0f}s"
            raise _RetryableError(msg)
        if status in _RETRYABLE_STATUS:
            msg = f"HTTP {status}"
            raise _RetryableError(msg)
        msg = f"{response.url}: unexpected HTTP {status}"
        raise FetchError(msg)

    def _finish(
        self,
        request_id: int,
        outcome: Outcome,
        *,
        status: int | None = None,
        error_class: str | None = None,
    ) -> None:
        with self._manifest.transaction():
            self._manifest.requests.finish(
                request_id,
                RequestResult(outcome=outcome, status=status, error_class=error_class),
            )
