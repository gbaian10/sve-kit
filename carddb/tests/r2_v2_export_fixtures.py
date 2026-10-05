"""Synthetic export-offline roots written by the real preview writer."""

from copy import deepcopy
from datetime import UTC, datetime, timedelta
from io import BytesIO
from shutil import copytree
from typing import TYPE_CHECKING

import pytest
from PIL import Image

from sve_carddb.image_variants import build_variants
from sve_carddb.snapshot.export import Batch, export_snapshot
from sve_carddb.snapshot.media import prepare_media
from sve_carddb.snapshot.preview import Roots, write_preview
from sve_carddb.snapshot.preview.media_state import reserve
from sve_carddb.snapshot.profiles import MEDIA

from .test_image_variants import source
from .test_snapshot_preview_images import PublicImages
from .test_snapshot_preview_images import images as images  # ruff: ignore[useless-import-alias] -- share one decoded synthetic library per module

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.snapshot.export import Brotli
    from sve_carddb.snapshot.project import Projection


def export(
    images: PublicImages,
    roots: Roots,
    *,
    step: int,
    projection: Projection | None = None,
    library: Path | None = None,
    brotli: Brotli | None = None,
) -> None:
    """Run the export-offline sealing steps: reserve, compare, write, commit."""
    moment = datetime(2026, 10, 4, 1, 2, 3, tzinfo=UTC) + timedelta(seconds=step)
    batch = Batch(
        "preview-" + moment.strftime("%Y%m%dT%H%M%SZ") + "-0001",
        moment.strftime("%Y-%m-%dT%H:%M:%SZ"),
        ("jp",),
    )
    revision, previous = reserve(roots)
    plan = prepare_media(
        projection or images.projection,
        library or images.library,
        revision=revision,
        previous=previous,
    )
    snapshot = export_snapshot(
        plan.projection, images.ownership, batch, format_version=MEDIA, brotli=brotli
    )
    write_preview(
        snapshot,
        roots,
        {},
        brotli=brotli,
        image_source=library or images.library,
        media_plan=plan,
    )


@pytest.fixture
def roots(tmp_path: Path) -> Roots:
    return Roots(tmp_path / "preview", tmp_path / "private")


@pytest.fixture(scope="module")
def art_changed(
    images: PublicImages, tmp_path_factory: pytest.TempPathFactory
) -> PublicImages:
    """Only the two art sizes change, as after a crop correction."""
    root = tmp_path_factory.mktemp("art-changed")
    copytree(images.library, root / "library")
    stream = BytesIO()
    Image.new("RGB", (160, 224), (200, 30, 70)).save(stream, format="PNG")
    output = build_variants(
        source(stream.getvalue()), blob_root=root / "library", cache_root=root / "cache"
    )
    changed = {v.size_key: v for v in output.variants}
    view = deepcopy(images.projection)
    for row in view.tables["image_variant"]:
        size = str(row["size_key"])
        if size.startswith("art_"):
            new = changed[size]
            row.update(
                path=new.path, width=new.width, height=new.height, bytes=new.bytes
            )
    return PublicImages(view, images.ownership, root / "library")
