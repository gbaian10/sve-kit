from typing import TYPE_CHECKING

import pytest

from sve_carddb.manifest import Manifest

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture
def manifest(tmp_path: Path) -> Manifest:
    return Manifest.open(tmp_path / "manifest" / "manifest.sqlite")
