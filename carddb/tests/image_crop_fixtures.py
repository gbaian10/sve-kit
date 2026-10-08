"""Small synthetic crop files written straight to a working tree."""

import json
from types import MappingProxyType
from typing import TYPE_CHECKING

import pytest

from sve_carddb.images.crops import FILE, ImageCrops

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.ingest.archive.source_archive import Descriptor


@pytest.fixture(scope="session")
def empty_crops() -> ImageCrops:
    return ImageCrops(MappingProxyType({}))


def record(descriptor: Descriptor) -> dict[str, JsonValue]:
    return {
        "source_key": descriptor.source_key,
        "source_sha256": descriptor.raw_sha256[7:],
        "left": 4,
        "top": 24,
        "width": 64,
        "height": 48,
        "reason": "Synthetic shifted crop",
        "region": "jp",
        "card_no": "TEST-001",
    }


def install(root: Path, records: list[dict[str, JsonValue]]) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / FILE).write_text(
        json.dumps(records, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )
