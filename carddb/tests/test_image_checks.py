"""One command reuses unique blobs, while new commands and changed files recheck them."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb import image_checks, image_variants
from sve_carddb.image_assets import (
    PreviewRoots,
    build_regional_assets,
    verify_asset_sources,
)
from sve_carddb.image_checks import ImageChecks
from sve_carddb.image_variants import build_variants
from sve_carddb.snapshot.values import digest

from .test_image_assets import frozen as frozen  # ruff: ignore[useless-import-alias] -- register the shared immutable image fixture
from .test_image_variants import png, source

if TYPE_CHECKING:
    from pathlib import Path

    from PIL import Image

    from sve_carddb.frozen_sources import FrozenSources
    from sve_carddb.image_crops import ImageCrops


def test_unique_source_decode_and_shared_variant_inspection(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    checks = ImageChecks()
    original = image_variants._decode
    decoded = 0

    def decode(raw: bytes) -> Image.Image:
        nonlocal decoded
        decoded += 1
        return original(raw)

    monkeypatch.setattr(image_variants, "_decode", decode)
    item = source(png(80, 112))
    first = build_variants(
        item, blob_root=tmp_path / "blobs", cache_root=tmp_path / "cache", checks=checks
    )
    second = build_variants(
        replace(item, image_id="img:jp:other"),
        blob_root=tmp_path / "blobs",
        cache_root=tmp_path / "cache",
        checks=checks,
    )
    assert decoded == 1
    assert second.cache_hit
    assert all(v.image_id == "img:jp:other" for v in second.variants)
    hashed = 0

    def counted(raw: bytes) -> str:
        nonlocal hashed
        hashed += 1
        return digest(raw)

    monkeypatch.setattr(image_checks, "digest", counted)
    path = tmp_path / "blobs" / first.variants[0].path
    assert checks.inspect(path) == checks.inspect(path)
    assert hashed == 1
    ImageChecks().inspect(path)
    assert hashed == 2
    path.write_bytes(b"changed external blob")
    with pytest.raises(OSError, match="cannot identify image"):
        checks.inspect(path)
    assert hashed == 3


def test_image_stages_share_the_verified_source_batch(
    frozen: FrozenSources,
    empty_crops: ImageCrops,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    checks = ImageChecks()
    roots = PreviewRoots(tmp_path / "blobs", tmp_path / "cache")
    assets = build_regional_assets(
        frozen, roots, region="jp", crops=empty_crops, checks=checks
    )
    assert checks.batch(frozen.root, frozen.store_id, frozen.batch_id) is frozen

    def unexpected(*_args: object, **_kwargs: object) -> None:
        pytest.fail("Image verification must reuse this command's verified batch")

    monkeypatch.setattr(image_checks, "FrozenSources", unexpected)
    verify_asset_sources(
        assets, {frozen.store_id: frozen.root}, crops=empty_crops, checks=checks
    )


def test_shared_checks_fill_each_cache_root(tmp_path: Path) -> None:
    checks = ImageChecks()
    item = source(png(80, 112))
    blobs = tmp_path / "blobs"
    first_cache = tmp_path / "first-cache"
    second_cache = tmp_path / "second-cache"
    first = build_variants(item, blob_root=blobs, cache_root=first_cache, checks=checks)
    second = build_variants(
        item, blob_root=blobs, cache_root=second_cache, checks=checks
    )
    assert not first.cache_hit
    assert not second.cache_hit
    assert first.variants == second.variants
    assert list((first_cache / "image-variants").glob("*.json"))
    assert list((second_cache / "image-variants").glob("*.json"))
    assert build_variants(
        item, blob_root=blobs, cache_root=second_cache, checks=checks
    ).cache_hit
