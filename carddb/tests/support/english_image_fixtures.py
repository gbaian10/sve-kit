"""A sealed synthetic EN image batch independent of text and database fixtures."""

import shutil
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.regions import SourceRegion
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.source_archive import ArchiveStore, Scope, seal_batch
from sve_carddb.parse.pages import official_en
from sve_carddb.parse.pages.official_jp import image_url

from ..images.test_image_variants import png
from ..ingest.test_source_archive import _put, _resource

if TYPE_CHECKING:
    from pathlib import Path

FRONT = "/synthetic/front .png"
BACK = "/synthetic/back.png"


@dataclass(frozen=True)
class EnglishImages:
    images: FrozenSources


@pytest.fixture(scope="module")
def english_images(
    tmp_path_factory: pytest.TempPathFactory,
    image_archive_template: tuple[Path, str, str],
) -> EnglishImages:
    base = tmp_path_factory.mktemp("english-images")
    original, store_id, _ = image_archive_template
    shutil.copytree(original.parent, base / "input")
    data_root = base / "input/data"
    store = ArchiveStore(
        data_root,
        data_root / "manifest/manifest.sqlite",
        data_root / "manifest/.lock",
        base / "input/archive",
        store_id,
    )
    for index, src in enumerate((FRONT, BACK)):
        raw = png(80, 112)
        _put(
            store,
            replace(
                _resource(
                    image_url(src, official_en.card_url("synthetic")),
                    f"raw/en-{index}.png",
                    raw,
                ),
                region=SourceRegion.EN,
            ),
            raw,
        )
    pin = seal_batch(store, scope=(Scope(provider="en", kind="image"),))
    return EnglishImages(FrozenSources(store.root, store_id, pin.batch_id))
