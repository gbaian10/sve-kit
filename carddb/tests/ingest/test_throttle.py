from typing import TYPE_CHECKING

import pytest

from sve_carddb.ingest.http.throttle import CircuitBreaker, CircuitOpenError, Throttle

if TYPE_CHECKING:
    from ..conftest import FakeClock


def throttle(clock: FakeClock, jitter_draw: float = 0.0) -> Throttle:
    return Throttle(2.5, 0.5, clock=clock, sleep=clock.sleep, rng=lambda: jitter_draw)


async def test_first_request_goes_immediately(clock: FakeClock) -> None:
    await throttle(clock).wait()
    assert clock.sleeps == []


async def test_requests_are_spaced_by_interval_plus_jitter(clock: FakeClock) -> None:
    gate = throttle(clock, jitter_draw=0.5)
    await gate.wait()
    await gate.wait()
    await gate.wait()
    assert clock.sleeps == [2.75, 2.75]


async def test_time_already_passed_counts_toward_the_gap(clock: FakeClock) -> None:
    gate = throttle(clock)
    await gate.wait()
    clock.now += 2.0
    await gate.wait()
    assert clock.sleeps == [pytest.approx(0.5)]


async def test_no_sleep_when_the_gap_has_passed(clock: FakeClock) -> None:
    gate = throttle(clock)
    await gate.wait()
    clock.now += 10.0
    await gate.wait()
    assert clock.sleeps == []


async def test_not_before_wins_when_later(clock: FakeClock) -> None:
    gate = throttle(clock)
    await gate.wait()
    await gate.wait(not_before=clock.now + 60.0)
    assert clock.sleeps == [60.0]


async def test_interval_wins_when_not_before_is_earlier(clock: FakeClock) -> None:
    gate = throttle(clock)
    await gate.wait()
    await gate.wait(not_before=clock.now + 1.0)
    assert clock.sleeps == [2.5]


def test_rejects_negative_settings() -> None:
    with pytest.raises(ValueError, match="negative"):
        Throttle(-1.0, 0.0)


def test_breaker_trips_on_consecutive_failures() -> None:
    breaker = CircuitBreaker(3)
    breaker.record_failure("a")
    breaker.record_failure("b")
    with pytest.raises(CircuitOpenError, match="3 consecutive failures, last: c"):
        breaker.record_failure("c")


def test_success_resets_the_breaker() -> None:
    breaker = CircuitBreaker(2)
    breaker.record_failure("a")
    breaker.record_success()
    breaker.record_failure("b")
    with pytest.raises(CircuitOpenError):
        breaker.record_failure("c")
