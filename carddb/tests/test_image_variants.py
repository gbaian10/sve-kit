"""Offline image recipe tests using synthetic pixels only."""

import hashlib
from dataclasses import replace
from io import BytesIO
from typing import TYPE_CHECKING

import pytest
from PIL import Image, ImageCms, ImageDraw, features

from sve_carddb import image_variants
from sve_carddb.image_variants import (
    DEFAULT_RECIPE,
    CropBox,
    CropOverride,
    ImageSource,
    ImageVariantError,
    Recipe,
    build_variants,
)

if TYPE_CHECKING:
    from pathlib import Path

RAW_SRC = "../cards/card image.png?edition=jp"


def png(
    width: int,
    height: int,
    *,
    alpha: bool = False,
    orientation: int | None = None,
    icc: bool = False,
) -> bytes:
    mode = "RGBA" if alpha else "RGB"
    image = Image.new(mode, (width, height), (24, 75, 131, 0 if alpha else 255))
    draw = ImageDraw.Draw(image)
    draw.rectangle(
        (width // 5, height // 5, 4 * width // 5, 4 * height // 5), fill="red"
    )
    draw.line((0, height - 1, width - 1, 0), fill="white", width=2)
    options: dict[str, bytes] = {}
    if orientation is not None:
        exif = image.getexif()
        exif[274] = orientation
        options["exif"] = exif.tobytes()
    if icc:
        options["icc_profile"] = ImageCms.ImageCmsProfile(
            ImageCms.createProfile("sRGB")
        ).tobytes()
    buffer = BytesIO()
    image.save(buffer, format="PNG", **options)
    return buffer.getvalue()


def source(data: bytes, *, image_id: str = "img:jp:1") -> ImageSource:
    return ImageSource(
        image_id=image_id,
        source_bytes=data,
        source_sha256=hashlib.sha256(data).hexdigest(),
        source_src_raw=RAW_SRC,
        asset_kind="sve_card",
        origin="official",
        publication_state="approved",
        availability="available",
    )


def build(
    item: ImageSource,
    root: Path,
    *,
    override: CropOverride | None = None,
    recipe: Recipe = DEFAULT_RECIPE,
) -> image_variants.VariantSet:
    return build_variants(
        item,
        blob_root=root / "blobs",
        cache_root=root / "cache",
        override=override,
        recipe=recipe,
    )


def test_portrait_sizes_crop_and_content_paths(tmp_path: Path) -> None:
    item = source(png(459, 641))
    result = build(item, tmp_path)
    assert result.source_src_raw == RAW_SRC
    assert result.crop_box == CropBox(36, 89, 384, 288)
    assert [(v.size_key, v.width, v.height) for v in result.variants] == [
        ("card_s", 128, 179),
        ("card_m", 320, 447),
        ("card_l", 459, 641),
        ("art_s", 160, 120),
        ("art_m", 384, 288),
    ]
    for variant in result.variants:
        data = (tmp_path / "blobs" / variant.path).read_bytes()
        assert variant.path == (
            f"images/sha256/{variant.sha256[:2]}/{variant.sha256}.webp"
        )
        assert variant.sha256 == hashlib.sha256(data).hexdigest()
        assert variant.bytes == len(data)
        assert variant.format == "webp"
        assert variant.is_original is False
        assert variant.recipe_version == result.recipe_version
        with Image.open(BytesIO(data)) as decoded:
            assert decoded.format == "WEBP"
            assert decoded.size == (variant.width, variant.height)
            assert not {"icc_profile", "exif", "xmp"} & decoded.info.keys()


def test_landscape_uses_actual_dimensions_without_art(tmp_path: Path) -> None:
    result = build(source(png(641, 459), image_id="img:spell"), tmp_path)
    assert result.crop_box is None
    assert [(v.size_key, v.width, v.height) for v in result.variants] == [
        ("card_s", 179, 128),
        ("card_m", 447, 320),
        ("card_l", 641, 459),
    ]


def test_square_and_tall_sources_follow_portrait_limits(tmp_path: Path) -> None:
    square = build(source(png(20, 20)), tmp_path / "square")
    assert len(square.variants) == 5
    tall = build(source(png(100, 1000)), tmp_path / "tall")
    assert [(v.width, v.height) for v in tall.variants[:3]] == [
        (18, 179),
        (45, 447),
        (64, 641),
    ]


def test_high_resolution_and_small_sources_do_not_upscale(tmp_path: Path) -> None:
    large = build(source(png(918, 1282)), tmp_path / "large")
    assert large.crop_box == CropBox(73, 179, 768, 576)
    assert [(v.width, v.height) for v in large.variants] == [
        (128, 179),
        (320, 447),
        (459, 641),
        (160, 120),
        (384, 288),
    ]
    small = build(source(png(100, 140)), tmp_path / "small")
    assert [(v.width, v.height) for v in small.variants] == [
        (100, 140),
        (100, 140),
        (100, 140),
        (84, 63),
        (84, 63),
    ]
    assert len({v.path for v in small.variants}) == 2


def test_exif_orientation_decides_landscape_after_transpose(tmp_path: Path) -> None:
    result = build(source(png(459, 641, orientation=6)), tmp_path)
    assert (result.source_width, result.source_height) == (641, 459)
    assert [v.size_key for v in result.variants] == ["card_s", "card_m", "card_l"]


def test_alpha_and_icc_metadata_are_handled_explicitly(tmp_path: Path) -> None:
    transparent = build(source(png(80, 112, alpha=True)), tmp_path / "alpha")
    with Image.open(
        tmp_path / "alpha" / "blobs" / transparent.variants[0].path
    ) as image:
        assert image.mode == "RGBA"
        assert image.getchannel("A").getpixel((0, 0)) == 0
    profiled = build(source(png(80, 112, icc=True)), tmp_path / "icc")
    with Image.open(tmp_path / "icc" / "blobs" / profiled.variants[0].path) as image:
        assert "icc_profile" not in image.info

    broken_profile = BytesIO()
    Image.new("RGB", (80, 112)).save(
        broken_profile, format="PNG", icc_profile=b"invalid profile"
    )
    with pytest.raises(ImageVariantError, match="ICC profile"):
        build(source(broken_profile.getvalue()), tmp_path / "broken-icc")


def test_two_faces_keep_separate_image_ids(tmp_path: Path) -> None:
    data = png(80, 112)
    front = build(source(data, image_id="front"), tmp_path)
    back = build(source(data, image_id="back"), tmp_path)
    assert not front.cache_hit
    assert back.cache_hit
    assert {v.image_id for v in front.variants} == {"front"}
    assert {v.image_id for v in back.variants} == {"back"}
    assert [v.path for v in front.variants] == [v.path for v in back.variants]


def test_cache_skips_encoding_and_clean_runs_match(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    item = source(png(200, 280))
    first = build(item, tmp_path / "first")
    other = build(item, tmp_path / "other")
    assert [v.sha256 for v in first.variants] == [v.sha256 for v in other.variants]
    assert [
        (tmp_path / "first" / "blobs" / v.path).read_bytes() for v in first.variants
    ] == [(tmp_path / "other" / "blobs" / v.path).read_bytes() for v in other.variants]

    def unexpected_encode(*_args: object, **_kwargs: object) -> bytes:
        pytest.fail("cache hit re-encoded the image")

    monkeypatch.setattr(image_variants, "_encode", unexpected_encode)
    cached = build(item, tmp_path / "first")
    assert cached.cache_hit
    assert cached.variants == first.variants


def test_cache_rejects_stale_dimensions(tmp_path: Path) -> None:
    item = source(png(80, 112))
    first = build(item, tmp_path)
    cache = next((tmp_path / "cache" / "image-variants").glob("*.json"))
    entry = cache.read_text()
    cache.write_text(entry.replace('"width":80', '"width":81', 1))
    rerun = build(item, tmp_path)
    assert not rerun.cache_hit
    assert rerun.variants == first.variants


def test_recipe_change_keeps_old_content_addressed_blobs(tmp_path: Path) -> None:
    item = source(png(200, 280))
    first = build(item, tmp_path)
    old = {v.path: (tmp_path / "blobs" / v.path).read_bytes() for v in first.variants}
    changed = build(item, tmp_path, recipe=Recipe(quality=50))
    assert changed.recipe_version != first.recipe_version
    assert changed.variants[2].sha256 != first.variants[2].sha256
    assert all(
        (tmp_path / "blobs" / path).read_bytes() == data for path, data in old.items()
    )
    assert len(list((tmp_path / "cache" / "image-variants").glob("*.json"))) == 2


def test_crop_override_is_tied_to_source_and_cannot_fallback(tmp_path: Path) -> None:
    item = source(png(100, 140))
    override = CropOverride(item.image_id, item.source_sha256, 4, 10, 64, 48, "face")
    result = build(item, tmp_path / "valid", override=override)
    assert result.crop_box == CropBox(4, 10, 64, 48)
    assert [(v.width, v.height) for v in result.variants[-2:]] == [(64, 48)] * 2
    invalid = (
        replace(override, source_sha256="0" * 64),
        replace(override, width=60),
        replace(override, left=90),
        replace(override, reason=" "),
    )
    for index, bad in enumerate(invalid):
        root = tmp_path / f"bad-{index}"
        with pytest.raises(ImageVariantError, match="override"):
            build(item, root, override=bad)
        assert not (root / "blobs").exists()
    landscape = source(png(140, 100))
    with pytest.raises(ImageVariantError, match="landscape"):
        build(landscape, tmp_path / "landscape", override=override)


def test_bad_images_and_unapproved_sources_write_nothing(tmp_path: Path) -> None:
    damaged = source(b"not a PNG")
    with pytest.raises(ImageVariantError, match="decode"):
        build(damaged, tmp_path / "damaged")
    tiny = source(png(3, 4))
    with pytest.raises(ImageVariantError, match="too small"):
        build(tiny, tmp_path / "tiny")
    good = source(png(80, 112))
    for index, bad in enumerate(
        (
            replace(good, source_sha256="0" * 64),
            replace(good, publication_state="pending"),
            replace(good, availability="missing"),
            replace(good, asset_kind="digital"),
        )
    ):
        root = tmp_path / f"blocked-{index}"
        with pytest.raises(ImageVariantError):
            build(bad, root)
        assert not (root / "blobs").exists()


def test_wrong_encoder_version_is_rejected(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    original = features.version

    def wrong_webp(feature: str) -> str | None:
        return "0.0.0" if feature == "webp" else original(feature)

    monkeypatch.setattr(features, "version", wrong_webp)
    with pytest.raises(ImageVariantError, match="differs from the pinned"):
        build(source(png(80, 112)), tmp_path)
    assert not (tmp_path / "blobs").exists()


def test_existing_corrupt_blob_is_never_overwritten(tmp_path: Path) -> None:
    item = source(png(80, 112))
    result = build(item, tmp_path)
    path = tmp_path / "blobs" / result.variants[0].path
    path.write_bytes(b"corrupt")
    with pytest.raises(ImageVariantError, match="corrupt"):
        build(item, tmp_path)
    assert path.read_bytes() == b"corrupt"
