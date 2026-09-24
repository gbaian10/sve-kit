from typing import TYPE_CHECKING

import pytest

from sve_carddb.manifest import Manifest

if TYPE_CHECKING:
    from pathlib import Path


class FakeClock:
    """A monotonic clock whose sleep only advances time."""

    def __init__(self) -> None:
        self.now = 1000.0
        self.sleeps: list[float] = []

    def __call__(self) -> float:
        return self.now

    async def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.now += seconds


@pytest.fixture
def clock() -> FakeClock:
    return FakeClock()


@pytest.fixture
def manifest(tmp_path: Path) -> Manifest:
    return Manifest.open(tmp_path / "manifest" / "manifest.sqlite")
