"""Politeness controls: a minimum gap between requests, and a circuit breaker."""

import asyncio
import random
import time
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Awaitable, Callable


class CircuitOpenError(RuntimeError):
    """Too many consecutive failures; the whole run must stop."""


class Throttle:
    """Keep at least `interval` (+ random jitter) seconds between requests.

    Every actual HTTP request goes through `wait`, including retries and
    redirect hops, so no code path can send faster than the limit.
    """

    def __init__(
        self,
        interval: float,
        jitter: float,
        *,
        clock: Callable[[], float] = time.monotonic,
        sleep: Callable[[float], Awaitable[None]] = asyncio.sleep,
        rng: Callable[[], float] = random.random,  # ruff: ignore[suspicious-non-cryptographic-random-usage] -- timing jitter, not security
    ) -> None:
        """Configure the gap; `clock`, `sleep` and `rng` are replaceable for tests."""
        if interval < 0 or jitter < 0:
            msg = "interval and jitter must not be negative"
            raise ValueError(msg)
        self._interval = interval
        self._jitter = jitter
        self._clock = clock
        self._sleep = sleep
        self._rng = rng
        self._last: float | None = None

    async def wait(self, not_before: float | None = None) -> None:
        """Sleep until the next request may go out, then claim that slot.

        `not_before` (a `clock` value) pushes the slot later, e.g. for
        `Retry-After`; the later of the two limits wins.
        """
        now = self._clock()
        ready = now
        if self._last is not None:
            ready = self._last + self._interval + self._jitter * self._rng()
        if not_before is not None:
            ready = max(ready, not_before)
        if ready > now:
            await self._sleep(ready - now)
        self._last = self._clock()


class CircuitBreaker:
    """Stop the run after `threshold` consecutive failures."""

    def __init__(self, threshold: int) -> None:
        """Trip after `threshold` failures in a row."""
        if threshold < 1:
            msg = "threshold must be at least 1"
            raise ValueError(msg)
        self._threshold = threshold
        self._failures = 0

    def record_success(self) -> None:
        """Reset the count."""
        self._failures = 0

    def record_failure(self, reason: str) -> None:
        """Count a failure; raise `CircuitOpenError` once the threshold is reached."""
        self._failures += 1
        if self._failures >= self._threshold:
            msg = f"{self._failures} consecutive failures, last: {reason}"
            raise CircuitOpenError(msg)
