"""Sealed invented EN images retain regional conversion and cache checks."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.images.assets import PreviewRoots, build_regional_assets

from ..support.english_image_fixtures import english_images as english_images  # ruff: ignore[useless-import-alias] -- reuse one sealed EN baseline

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.images.crops import ImageCrops

    from ..support.english_image_fixtures import EnglishImages


def test_en_conversion_reuses_verified_cache(
    english_images: EnglishImages, empty_crops: ImageCrops, tmp_path: Path
) -> None:
    roots = PreviewRoots(tmp_path / "library", tmp_path / "cache")
    first = build_regional_assets(
        english_images.images, roots, region="en", crops=empty_crops, workers=2
    )
    assert len(first.images) == 2
    reused = build_regional_assets(
        english_images.images, roots, region="en", crops=empty_crops, workers=2
    )
    assert all(item.region == "en" and item.result.cache_hit for item in reused.images)
    assert [item.result.variants for item in first.images] == [
        item.result.variants for item in reused.images
    ]


def test_en_declared_region_cannot_cross_jp(
    english_images: EnglishImages, empty_crops: ImageCrops, tmp_path: Path
) -> None:
    roots = PreviewRoots(tmp_path / "library", tmp_path / "cache")
    with pytest.raises(ValueError, match="exclusively JP"):
        build_regional_assets(
            english_images.images, roots, region="jp", crops=empty_crops
        )
    assert not roots.preview.exists()
