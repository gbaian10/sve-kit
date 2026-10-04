"""Small synthetic crop shards with isolated immutable Git pins."""

import json
from typing import TYPE_CHECKING

import pytest

from sve_carddb.image_crops import load_image_crops
from sve_carddb.snapshot.values import canonical, digest

from .adoption_fixtures import commit, git

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.image_crops import ImageCrops
    from sve_carddb.source_archive import Descriptor

SHARD = "image-crops/TEST/001.yaml"


@pytest.fixture(scope="session")
def empty_crops(tmp_path_factory: pytest.TempPathFactory) -> ImageCrops:
    repo = tmp_path_factory.mktemp("empty-crop-adoptions")
    (repo / "keep").write_text("Synthetic empty crop closure\n", encoding="utf-8")
    return load_image_crops(repo / "authored", authored_revision=initialize(repo))


def write(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, ensure_ascii=False, sort_keys=True) + "\n", encoding="utf-8"
    )


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
    write(
        root / SHARD,
        {
            "image_crop_format": 2,
            "kind": "crop_override_shard",
            "records": sorted(
                records,
                key=lambda r: (
                    str(r["region"]),
                    str(r["card_no"]),
                    str(r["source_key"]),
                    str(r["source_sha256"]),
                ),
            ),
        },
    )


def initialize(repo: Path) -> str:
    git(repo, "init", "-q")
    return commit(repo)


def version(source_key: str, raw_hash: str) -> str:
    return (
        "src:v1:"
        + digest(canonical({"source_key": source_key, "raw_sha256": raw_hash}))[7:]
    )
