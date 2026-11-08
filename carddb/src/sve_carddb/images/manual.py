"""Convert maintainer-supplied master PNGs of manual printings into card variants."""

import hashlib
from dataclasses import dataclass
from time import perf_counter
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, digest
from sve_carddb.images.variants import ImageSource, VariantSet, build_variants

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.manual_printings.loader import ManualSnapshot
    from sve_carddb.domains.manual_printings.records import ManualPrintingData
    from sve_carddb.images.assets import PreviewRoots
    from sve_carddb.images.checks import ImageChecks


def manual_image_id(printing_id: str, sha256: str | None) -> str:
    """Bind the public asset to the printing and the exact master, never a file name."""
    return (
        "img:manual:"
        + digest(canonical({"printing_id": printing_id, "sha256": sha256}))[7:]
    )


def manual_source_src(data: ManualPrintingData) -> str:
    """The master's hash stands in for an img src; a missing master names the record."""
    return "manual:" + data.slug if data.image is None else data.image.sha256


@dataclass(frozen=True)
class ManualImage:
    printing: ManualPrintingData
    result: VariantSet
    elapsed_seconds: float


@dataclass(frozen=True)
class ManualImages:
    snapshot: ManualSnapshot
    images: tuple[ManualImage, ...]

    def by_printing(self) -> dict[str, ManualImage]:
        """Index converted masters by their printing ID."""
        return {item.printing.id: item for item in self.images}


def master_files(directory: Path) -> dict[str, Path]:
    """Map every PNG below the directory by content hash; file names carry no meaning."""
    if not directory.is_absolute() or directory.is_symlink() or not directory.is_dir():
        raise ValueError("Manual image directory must be an absolute real directory")
    result: dict[str, Path] = {}
    for path in sorted(directory.rglob("*.png")):
        if path.is_symlink():
            raise ValueError("Manual image directory must not contain symlinks")
        result.setdefault(hashlib.sha256(path.read_bytes()).hexdigest(), path)
    return result


def build_manual_assets(
    snapshot: ManualSnapshot,
    masters: Path | None,
    roots: PreviewRoots,
    *,
    checks: ImageChecks | None = None,
) -> ManualImages:
    """Encode each approved master once; dimensions must match the authored record."""
    records = [data for data in snapshot.printings() if data.image is not None]
    if not records:
        return ManualImages(snapshot, ())
    if masters is None:
        raise ValueError("Manual printings with images require --manual-image-dir")
    roots.validate((masters,))
    files = master_files(masters)
    images: list[ManualImage] = []
    for data in records:
        image = data.image
        assert image is not None
        hexdigest = image.sha256.removeprefix("sha256:")
        path = files.get(hexdigest)
        if path is None:
            raise ValueError(f"Missing master PNG for manual printing {data.slug}")
        began = perf_counter()
        result = build_variants(
            ImageSource(
                manual_image_id(data.id, image.sha256),
                path,
                hexdigest,
                manual_source_src(data),
                "sve_card",
                "approved",
                "available",
            ),
            blob_root=roots.preview,
            cache_root=roots.cache,
            checks=checks,
        )
        if (result.source_width, result.source_height) != (image.width, image.height):
            raise ValueError(
                f"Master PNG dimensions differ from the authored record: {data.slug}"
            )
        images.append(ManualImage(data, result, perf_counter() - began))
    return ManualImages(snapshot, tuple(images))
