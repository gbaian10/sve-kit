"""Versioned replay engineering budgets with a bounded in-process watchdog."""

import resource
import signal
import threading
import time
from contextlib import contextmanager
from typing import TYPE_CHECKING, Annotated, Literal

from pydantic import Field, field_validator

from sve_carddb.registry.records import RecordData

if TYPE_CHECKING:
    from collections.abc import Iterator
    from types import FrameType

SECOND_LIMIT = 1800
RSS_LIMIT = 6 * 1024**3
GOAL_SECONDS = 1200
GOAL_RSS = 4 * 1024**3


class Budget(RecordData):
    format: Literal[1] = 1
    wall_seconds: Annotated[int, Field(strict=True, ge=0, le=SECOND_LIMIT)] = (
        SECOND_LIMIT
    )
    rss_bytes: Annotated[int, Field(strict=True, ge=0, le=RSS_LIMIT)] = RSS_LIMIT

    @field_validator("format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Replay budget format must be integer one")
        return value


class ReplayBudgetExceededError(ValueError):
    """A hard budget refusal never returns a partially successful result."""


class Monitor:
    def __init__(self, budget: Budget) -> None:
        self.budget = budget
        self.started = time.monotonic()
        self.optimization_required = False

    def estimate(
        self, effects: float, flavors: float, *, retained_bytes: int = 0
    ) -> None:
        """More shards do not add work; distinct historical groups do."""
        estimate = 60 + 1.5 * (160 * effects + 382 * flavors)
        memory = max(
            resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * 1024,
            int(retained_bytes * 1.3),
        )
        self.optimization_required = estimate > GOAL_SECONDS or memory > GOAL_RSS
        if estimate > self.budget.wall_seconds or memory > self.budget.rss_bytes:
            raise ReplayBudgetExceededError("replay_budget_exceeded")

    def check(self) -> None:
        """RSS includes native allocations and all retained global/group state."""
        elapsed = time.monotonic() - self.started
        rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * 1024
        self.optimization_required |= elapsed > GOAL_SECONDS or rss > GOAL_RSS
        if elapsed > self.budget.wall_seconds or rss > self.budget.rss_bytes:
            raise ReplayBudgetExceededError("replay_budget_exceeded")

    @contextmanager
    def watchdog(self) -> Iterator[None]:
        """Restore the caller's timer; no subprocess or automatic budget increase is used."""
        if threading.current_thread() is not threading.main_thread():
            raise ValueError("Replay watchdog requires the main thread")
        previous = signal.getsignal(signal.SIGALRM)
        timer = signal.getitimer(signal.ITIMER_REAL)
        started = time.monotonic()

        def alarm(_signum: int, _frame: FrameType | None) -> None:
            self.check()

        signal.signal(signal.SIGALRM, alarm)
        signal.setitimer(signal.ITIMER_REAL, 0.1, 0.1)
        try:
            self.check()
            yield
            self.check()
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
            signal.signal(signal.SIGALRM, previous)
            signal.setitimer(
                signal.ITIMER_REAL,
                max(0.000001, timer[0] - (time.monotonic() - started))
                if timer[0]
                else 0,
                timer[1],
            )
