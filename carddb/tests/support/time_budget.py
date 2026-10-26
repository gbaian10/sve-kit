"""Account for complete test time, with explicit reasons for shared slow preparation."""

from __future__ import annotations

import math
import os
import time
import warnings
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

import pytest

if TYPE_CHECKING:
    from collections.abc import Callable, Generator

    from _pytest.terminal import TerminalReporter


@dataclass(frozen=True)
class Budget:
    limit: float
    ci: bool


@dataclass
class Timing:
    elapsed: float = 0
    failed: bool = False
    exemptions: list[tuple[float, float, str]] = field(default_factory=list)


_BUDGET = pytest.StashKey[Budget]()
_TIMING = pytest.StashKey[Timing]()
_ITEM = pytest.StashKey[pytest.Item]()


def _reason(value: object, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise pytest.UsageError(f"{label} requires a non-empty reason string")
    return value.strip()


def slow_fixture[**P, R](*, reason: str) -> Callable[[Callable[P, R]], Callable[P, R]]:
    """Exempt a shared fixture's own setup/finalizers; place below @pytest.fixture."""
    checked = _reason(reason, "slow_fixture")

    def decorate(function: Callable[P, R]) -> Callable[P, R]:
        function.__dict__["sve_slow_reason"] = checked
        return function

    return decorate


def pytest_configure(config: pytest.Config) -> None:
    raw = os.environ.get("SVE_TEST_TIME_LIMIT", "30")
    try:
        limit = float(raw)
    except ValueError:
        raise pytest.UsageError(
            "SVE_TEST_TIME_LIMIT must be a positive finite number"
        ) from None
    if not math.isfinite(limit) or limit <= 0:
        raise pytest.UsageError("SVE_TEST_TIME_LIMIT must be a positive finite number")
    ci = os.environ.get("CI", "").strip().lower() not in {"", "0", "false", "no", "off"}
    config.stash[_BUDGET] = Budget(limit, ci)
    config.addinivalue_line(
        "markers",
        "slow(reason): allow a test over the time budget with a concrete reason",
    )


def pytest_collection_modifyitems(items: list[pytest.Item]) -> None:
    for item in items:
        for marker in item.iter_markers("slow"):
            _reason(marker.kwargs.get("reason"), f"{item.nodeid}: slow")


@pytest.hookimpl(tryfirst=True)
def pytest_runtest_setup(item: pytest.Item) -> None:
    item.config.stash[_ITEM] = item
    item.stash[_TIMING] = Timing()


def _exempt(config: pytest.Config, start: float, label: str) -> None:
    item = config.stash[_ITEM]
    item.stash[_TIMING].exemptions.append((start, time.perf_counter(), label))


@pytest.hookimpl(wrapper=True, tryfirst=True)
def pytest_fixture_setup(
    fixturedef: pytest.FixtureDef[object], request: pytest.FixtureRequest
) -> Generator[None, object, object]:
    reason = getattr(fixturedef.func, "sve_slow_reason", None)
    if reason is None:
        return (yield)
    reason = _reason(reason, f"fixture {fixturedef.argname}: slow_fixture")
    if fixturedef.scope == "function":
        raise pytest.UsageError(
            "slow_fixture requires a shared fixture; mark the test slow instead"
        )
    label = f"fixture {fixturedef.argname}: {reason}"
    finalizer_start: list[float] = []

    def finish() -> None:
        _exempt(request.config, finalizer_start[0], label)

    # LIFO brackets include the fixture's generator teardown and registered finalizers.
    fixturedef.addfinalizer(finish)
    start = time.perf_counter()
    try:
        return (yield)
    finally:
        _exempt(request.config, start, label)
        fixturedef.addfinalizer(lambda: finalizer_start.append(time.perf_counter()))


def _exempt_seconds(intervals: list[tuple[float, float, str]]) -> float:
    total = 0.0
    end = 0.0
    for start, stop, _ in sorted(intervals):
        total += max(0.0, stop - max(start, end))
        end = max(end, stop)
    return total


@pytest.hookimpl(wrapper=True)
def pytest_runtest_makereport(
    item: pytest.Item, call: pytest.CallInfo[None]
) -> Generator[None, pytest.TestReport, pytest.TestReport]:
    report = yield
    timing = item.stash[_TIMING]
    timing.elapsed += call.duration
    timing.failed |= report.failed
    if report.when != "teardown":
        return report
    budget = item.config.stash[_BUDGET]
    accounted = max(0.0, timing.elapsed - _exempt_seconds(timing.exemptions))
    for start, end, label in timing.exemptions:
        if end - start > budget.limit:
            report.user_properties.append(
                ("sve_time_budget", f"{end - start:.3f}s {label}")
            )
    marker = item.get_closest_marker("slow")
    if marker is not None and accounted > budget.limit:
        report.user_properties.append(
            (
                "sve_time_budget",
                f"{accounted:.3f}s {item.nodeid}: {marker.kwargs['reason']}",
            )
        )
    if (
        marker is not None
        or accounted <= budget.limit
        or timing.failed
        or report.skipped
    ):
        return report
    message = (
        f"Test time budget exceeded: {item.nodeid}: {accounted:.3f}s "
        f"(setup + call + teardown {timing.elapsed:.3f}s; limit {budget.limit:g}s). "
        'Use @pytest.mark.slow(reason="concrete reason"), or @slow_fixture(reason="concrete reason") '
        "below @pytest.fixture for shared preparation."
    )
    if budget.ci:
        report.outcome = "failed"
        report.longrepr = message
    else:
        warnings.warn(pytest.PytestWarning(message), stacklevel=2)
    return report


def pytest_terminal_summary(terminalreporter: TerminalReporter) -> None:
    entries = {
        str(value)
        for reports in terminalreporter.stats.values()
        for report in reports
        for name, value in getattr(report, "user_properties", ())
        if name == "sve_time_budget"
    }
    if entries:
        terminalreporter.section("slow time budget exemptions")
        for entry in sorted(entries):
            terminalreporter.write_line(entry)
