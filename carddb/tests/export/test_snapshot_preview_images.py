"""Synthetic public WebP bytes, state gates and preview activation failures."""

import shutil
from copy import deepcopy
from dataclasses import dataclass, replace
from io import BytesIO
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from PIL import Image

from sve_carddb.build import create_database
from sve_carddb.core.json import digest, integer, string
from sve_carddb.export.media import MediaPlan, prepare_media
from sve_carddb.export.media_images import image_blobs
from sve_carddb.export.preview import Roots, write_preview
from sve_carddb.export.transport import Ownership, export_snapshot
from sve_carddb.images.variants import build_variants

from ..images.test_image_variants import png, source
from ..support.snapshot_media_fixtures import media_rows
from ..support.snapshot_project_fixtures import populate, schema
from .test_snapshot_export import BATCH
from .test_snapshot_project import projected

if TYPE_CHECKING:
    from sve_carddb.export.project import Projection
    from sve_carddb.export.project.source import Record
    from sve_carddb.export.transport import Snapshot


@dataclass(frozen=True)
class PublicImages:
    projection: Projection
    ownership: Ownership
    library: Path

    def plan(self, tables: dict[str, list[Record]] | None = None) -> MediaPlan:
        projection = (
            self.projection
            if tables is None
            else replace(self.projection, tables=tables)
        )
        return prepare_media(projection, self.library, revision=1)

    def snapshot(self, tables: dict[str, list[Record]] | None = None) -> Snapshot:
        return export_snapshot(self.plan(tables).projection, self.ownership, BATCH)

    def tables(self) -> dict[str, list[Record]]:
        return deepcopy(self.projection.tables)


@pytest.fixture(scope="module")
def images(tmp_path_factory: pytest.TempPathFactory) -> PublicImages:
    root = tmp_path_factory.mktemp("public-image-library")
    result = build_variants(
        source(png(160, 224)), blob_root=root / "library", cache_root=root / "cache"
    )
    with create_database(schema()) as db:
        with db.transaction():
            populate(db)
        projection = projected(db)
        tables = (
            projection.tables
            | media_rows()
            | {
                "image_variant": [
                    {
                        "image_id": "image",
                        "size_key": v.size_key,
                        "format": v.format,
                        "path": v.path,
                        "width": v.width,
                        "height": v.height,
                        "bytes": v.bytes,
                    }
                    for v in sorted(result.variants, key=lambda item: item.size_key)
                ],
            }
        )
        tables["printing_image"][0].update(
            publication_state="approved", availability="available"
        )
        projection = replace(projection, tables=tables)
        return PublicImages(
            projection, Ownership.from_database(db, projection), root / "library"
        )


def test_text_writer_needs_no_image_library(
    images: PublicImages, tmp_path: Path
) -> None:
    snapshot = images.snapshot()
    roots = Roots(tmp_path / "preview", tmp_path / "private")
    write_preview(snapshot, roots, {})
    assert not (roots.preview / "images").exists()
    assert not (roots.private / "media-state.json").exists()


@pytest.mark.parametrize(
    "case",
    [
        "source",
        "format",
        "prefix",
        "path",
        "digital",
        "shared-metadata",
    ],
)
def test_public_asset_validation_counterexamples(
    images: PublicImages, case: str
) -> None:
    tables = images.tables()
    variant = tables["image_variant"][0]
    changes: dict[str, Record] = {
        "format": {"format": "png"},
        "prefix": {
            "path": "images/sha256/00/" + string(variant["path"]).split("/")[-1]
        },
        "path": {"path": "../source.png"},
    }
    if case in changes:
        variant.update(changes[case])
    elif case == "digital":
        tables["printing_image"] = []
    elif case == "shared-metadata":
        tables["image_variant"].append(
            variant | {"width": integer(variant["width"]) + 1}
        )
    message = {
        "source": "Preview images require an explicit asset source",
        "digital": "Only approved available printing images can be published",
        "shared-metadata": "Shared image blob metadata disagrees",
    }.get(case, "Preview image requires a content-addressed WebP path")
    with pytest.raises(ValueError, match=f"^{message}$"):
        list(image_blobs(tables, None if case == "source" else images.library))


@pytest.mark.parametrize("field", ["width", "height", "bytes"])
def test_valid_webp_must_match_manifest_dimensions_and_bytes(
    images: PublicImages, field: str
) -> None:
    tables = images.tables()
    variant = tables["image_variant"][0]
    path = string(variant["path"])
    raw = (images.library / path).read_bytes()
    assert digest(raw)[7:] == Path(path).stem
    assert len(raw) == integer(variant["bytes"])
    # Shared hashes must keep consistent metadata so the decoded/file-size guard is reached.
    for row in tables["image_variant"]:
        if row["path"] == path:
            row[field] = integer(row[field]) + 1
    message = "Preview image hash, bytes, format or dimensions mismatch"
    with pytest.raises(ValueError, match=f"^{message}$"):
        list(image_blobs(tables, images.library))


@pytest.mark.parametrize("kind", ["relative", "symlink"])
def test_image_input_root_requires_absolute_non_symlink_path(
    images: PublicImages,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    kind: str,
) -> None:
    if kind == "relative":
        monkeypatch.chdir(images.library.parent)
        root = Path(images.library.name)
    else:
        root = tmp_path / "linked-library"
        root.symlink_to(images.library, target_is_directory=True)
    assert root.is_dir()
    with pytest.raises(
        ValueError, match=r"^Preview image input must be an absolute non-symlink root$"
    ):
        list(image_blobs(images.projection.tables, root))


@pytest.mark.parametrize("case", ["corrupt", "missing", "symlink", "png"])
def test_actual_image_bytes_must_match_webp_metadata(
    images: PublicImages, tmp_path: Path, case: str
) -> None:
    tables = images.tables()
    library = tmp_path / "library"
    shutil.copytree(images.library, library)
    variant = tables["image_variant"][0]
    blob = library / string(variant["path"])
    raw = blob.read_bytes()
    if case == "corrupt":
        blob.write_bytes(raw + b"corrupt")
    elif case == "missing":
        blob.unlink()
    elif case == "symlink":
        blob.unlink()
        blob.symlink_to(images.library / string(variant["path"]))
    else:
        raw = png(8, 8)
        hashed = digest(raw)[7:]
        variant.update(
            {
                "path": f"images/sha256/{hashed[:2]}/{hashed}.webp",
                "bytes": len(raw),
                "width": 8,
                "height": 8,
            }
        )
        blob = library / string(variant["path"])
        blob.parent.mkdir(parents=True, exist_ok=True)
        blob.write_bytes(raw)
    with pytest.raises((ValueError, FileNotFoundError)):
        list(image_blobs(tables, library))


def test_equal_length_webp_tamper_requires_content_hash(
    images: PublicImages, tmp_path: Path
) -> None:
    library = tmp_path / "library"
    shutil.copytree(images.library, library)
    variant = images.projection.tables["image_variant"][0]
    blob = library / string(variant["path"])
    raw = blob.read_bytes()
    changed = raw[:-1] + bytes([raw[-1] ^ 1])
    assert len(changed) == integer(variant["bytes"])
    assert digest(changed) != digest(raw)
    # The size/decoder guards must accept the tamper to isolate the hash requirement.
    with Image.open(BytesIO(changed)) as decoded:
        decoded.load()
        assert decoded.format == "WEBP"
        assert decoded.size == (integer(variant["width"]), integer(variant["height"]))
    blob.write_bytes(changed)
    with pytest.raises(
        ValueError, match=r"^Preview image hash, bytes, format or dimensions mismatch$"
    ):
        list(image_blobs(images.projection.tables, library))
