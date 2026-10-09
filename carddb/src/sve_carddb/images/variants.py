"""Deterministic, content-addressed WebP variants for approved SVE card images."""

import hashlib
import os
import re
import tempfile
from dataclasses import asdict, dataclass, replace
from io import BytesIO
from pathlib import Path
from threading import RLock
from typing import TYPE_CHECKING, Literal

from PIL import Image, ImageCms, ImageOps, UnidentifiedImageError, features
from PIL import __version__ as pillow_version
from pydantic import BaseModel, ConfigDict, ValidationError

from sve_carddb.core.json import canonical, digest
from sve_carddb.images.checks import ImageChecks

if TYPE_CHECKING:
    from collections.abc import Iterable

    from pydantic import JsonValue

PILLOW_VERSION = "12.3.0"
LIBWEBP_VERSION = "1.6.0"
LITTLECMS_VERSION = "2.19"
_MAX_QUALITY = 100
_MAX_METHOD = 6
_HASH = re.compile(r"[0-9a-f]{64}\Z")


class ImageVariantError(ValueError):
    """The source, recipe, or existing output cannot produce valid variants."""


@dataclass(frozen=True, slots=True)
class SizeSpec:
    key: str
    purpose: Literal["card", "art"]
    max_width: int
    max_height: int


SIZES = (
    SizeSpec("card_s", "card", 128, 179),
    SizeSpec("card_m", "card", 320, 447),
    SizeSpec("card_l", "card", 459, 641),
    SizeSpec("art_s", "art", 160, 120),
    SizeSpec("art_m", "art", 384, 288),
)


@dataclass(frozen=True, slots=True)
class ImageSource:
    image_id: str
    source_bytes: bytes
    source_sha256: str
    source_src_raw: str
    asset_kind: Literal["sve_card", "digital"]
    publication_state: Literal["pending", "approved"]
    availability: Literal["available", "missing", "unfetched"]


@dataclass(frozen=True, slots=True)
class CropBox:
    left: int
    top: int
    width: int
    height: int

    @property
    def bounds(self) -> tuple[int, int, int, int]:
        """The half-open crop rectangle."""
        return (self.left, self.top, self.left + self.width, self.top + self.height)


@dataclass(frozen=True, slots=True)
class Recipe:
    quality: int = 82
    alpha_quality: int = 100
    method: int = 6
    exact_alpha: bool = True

    def definition(self) -> dict[str, JsonValue]:
        """Return every setting that can affect the encoded bytes."""
        return {
            "algorithm": "sve-webp-v2",
            "pillow": PILLOW_VERSION,
            "libwebp": LIBWEBP_VERSION,
            "littlecms": LITTLECMS_VERSION,
            "decode": "png-single-frame",
            "orientation": "exif-transpose-before-geometry",
            "color": "rgb-or-gray-icc-to-srgb-else-assume-srgb",
            "non_rgb": "palette-or-gray-to-rgb;16bit-gray-upper-byte",
            "icc_rendering_intent": "perceptual",
            "icc_flags": 0,
            "alpha": "preserve-rgba-exact",
            "metadata": "strip-icc-exif-xmp",
            "resize": "lanczos-no-reducing-gap",
            "rounding": "floor-x-plus-half-min-one",
            "crop": {
                "algorithm": "integer-4x3-v2",
                "portrait": {"left_percent": 8, "top_percent": 14, "width_percent": 84},
                "landscape": {
                    "left_percent": 17,
                    "top_percent": 4,
                    "width_percent": 65,
                },
                "override": "source-bound-integer-in-bounds-exact-4x3",
            },
            "sizes": [asdict(size) for size in SIZES],
            "lossless": False,
            "quality": self.quality,
            "alpha_quality": self.alpha_quality,
            "method": self.method,
            "exact_alpha": self.exact_alpha,
        }

    @property
    def version(self) -> str:
        """Hash the complete encoding and geometry recipe."""
        return digest(canonical(self.definition()))


DEFAULT_RECIPE = Recipe()


@dataclass(frozen=True, slots=True)
class ImageVariant:
    image_id: str
    size_key: str
    format: Literal["webp"]
    path: str
    width: int
    height: int
    bytes: int
    sha256: str
    recipe_version: str
    is_original: Literal[False] = False


@dataclass(frozen=True, slots=True)
class VariantSet:
    image_id: str
    source_src_raw: str
    source_sha256: str
    source_width: int
    source_height: int
    crop_box: CropBox
    recipe_version: str
    variants: tuple[ImageVariant, ...]
    cache_hit: bool


class _CachedVariant(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)

    size_key: str
    width: int
    height: int
    bytes: int
    sha256: str


class _CacheEntry(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)

    cache_key: str
    source_sha256: str
    recipe_version: str
    source_width: int
    source_height: int
    crop: list[int]
    variants: list[_CachedVariant]


def build_variants(
    source: ImageSource,
    *,
    blob_root: Path,
    cache_root: Path,
    override: CropBox | None = None,
    recipe: Recipe = DEFAULT_RECIPE,
    checks: ImageChecks | None = None,
) -> VariantSet:
    """Build or reuse WebP variants without reading any project data directory."""
    _validate_source(source)
    _validate_recipe(recipe)
    checks = checks or ImageChecks()
    key = (
        source.source_sha256,
        recipe.version,
        override,
        blob_root.resolve(),
        cache_root.resolve(),
    )
    with checks.lock:
        lock = checks.variant_locks.setdefault(key, RLock())
    with lock:
        previous = checks.variants.get(key)
        reused = previous is not None
        if previous is None:
            previous = _build_variants(
                source, blob_root, cache_root, override, recipe, checks
            )
            checks.variants[key] = previous
        return replace(
            previous,
            cache_hit=previous.cache_hit or reused,
            image_id=source.image_id,
            source_src_raw=source.source_src_raw,
            variants=tuple(
                replace(v, image_id=source.image_id) for v in previous.variants
            ),
        )


def _build_variants(
    source: ImageSource,
    blob_root: Path,
    cache_root: Path,
    override: CropBox | None,
    recipe: Recipe,
    checks: ImageChecks,
) -> VariantSet:
    image = _decode(source.source_bytes)
    checks.png["sha256:" + source.source_sha256] = "PNG", image.size
    crop = crop_box(image.width, image.height, override)
    cache_key = digest(
        canonical(
            {
                "source_sha256": source.source_sha256,
                "crop": list(crop.bounds),
                "recipe": recipe.definition(),
            }
        )
    ).removeprefix("sha256:")
    cache_path = cache_root / "image-variants" / f"{cache_key}.json"
    cached = _read_cache(
        cache_path, source, image, crop, recipe, blob_root, checks=checks
    )
    if cached is not None:
        return cached

    generated: list[ImageVariant] = []
    for size in SIZES:
        if size.purpose == "art":
            pixels = image.crop(crop.bounds)
            dimensions = _art_dimensions(crop, size)
        else:
            pixels = image
            dimensions = _card_dimensions(image.width, image.height, size)
        if pixels.size != dimensions:
            pixels = pixels.resize(
                dimensions, Image.Resampling.LANCZOS, reducing_gap=None
            )
        encoded = _encode(pixels, recipe)
        checksum = hashlib.sha256(encoded).hexdigest()
        path = _blob_path(checksum)
        _write_blob(blob_root / path, encoded, checksum)
        generated.append(
            ImageVariant(
                image_id=source.image_id,
                size_key=size.key,
                format="webp",
                path=path,
                width=dimensions[0],
                height=dimensions[1],
                bytes=len(encoded),
                sha256=checksum,
                recipe_version=recipe.version,
            )
        )

    result = VariantSet(
        image_id=source.image_id,
        source_src_raw=source.source_src_raw,
        source_sha256=source.source_sha256,
        source_width=image.width,
        source_height=image.height,
        crop_box=crop,
        recipe_version=recipe.version,
        variants=tuple(generated),
        cache_hit=False,
    )
    _write_cache(cache_path, result, cache_key)
    return result


def _validate_source(source: ImageSource) -> None:
    if not source.image_id or not source.source_src_raw:
        msg = "image identity and original source URL are required"
        raise ImageVariantError(msg)
    if source.asset_kind != "sve_card":
        msg = "digital images cannot enter SVE card variants"
        raise ImageVariantError(msg)
    if source.publication_state != "approved" or source.availability != "available":
        msg = "only approved, available SVE card images can have public variants"
        raise ImageVariantError(msg)
    digest = hashlib.sha256(source.source_bytes).hexdigest()
    if not _HASH.fullmatch(source.source_sha256) or digest != source.source_sha256:
        msg = "source SHA-256 does not match the frozen image bytes"
        raise ImageVariantError(msg)


def _validate_recipe(recipe: Recipe) -> None:
    if (
        pillow_version != PILLOW_VERSION
        or features.version("webp") != LIBWEBP_VERSION
        or features.version("littlecms2") != LITTLECMS_VERSION
    ):
        msg = "Pillow, libwebp, or LittleCMS differs from the pinned image recipe"
        raise ImageVariantError(msg)
    if (
        not 0 <= recipe.quality <= _MAX_QUALITY
        or not 0 <= recipe.alpha_quality <= _MAX_QUALITY
        or not 0 <= recipe.method <= _MAX_METHOD
    ):
        msg = "invalid WebP encoding parameters"
        raise ImageVariantError(msg)


def _decode(source_bytes: bytes) -> Image.Image:
    try:
        with Image.open(BytesIO(source_bytes)) as opened:
            if opened.format != "PNG" or getattr(opened, "n_frames", 1) != 1:
                msg = "source must be a single-frame PNG"
                raise ImageVariantError(msg)
            opened.load()
            oriented = ImageOps.exif_transpose(opened)
    except (Image.DecompressionBombError, OSError, UnidentifiedImageError) as exc:
        msg = "cannot decode source image"
        raise ImageVariantError(msg) from exc
    if oriented.width <= 0 or oriented.height <= 0:
        msg = "source dimensions must be positive"
        raise ImageVariantError(msg)
    profile = oriented.info.get("icc_profile")
    oriented = _normalize_depth(oriented)
    if profile is not None:
        oriented = _convert_icc(oriented, profile)
    has_alpha = "A" in oriented.getbands() or "transparency" in oriented.info
    mode = "RGBA" if has_alpha else "RGB"
    converted = oriented.convert(mode)
    return Image.frombytes(mode, converted.size, converted.tobytes())


def _normalize_depth(image: Image.Image) -> Image.Image:
    if image.mode in {"I;16", "I;16B", "I;16L"}:
        # Pillow's direct I;16-to-L conversion clips values above 255.
        return image.convert("I").point(lambda value: value / 256).convert("L")
    return image


def _convert_icc(image: Image.Image, profile: object) -> Image.Image:
    if not isinstance(profile, bytes):
        msg = "embedded ICC profile must contain bytes"
        raise ImageVariantError(msg)
    has_alpha = "A" in image.getbands() or "transparency" in image.info
    try:
        input_profile = ImageCms.getOpenProfile(BytesIO(profile))
        color_space = getattr(input_profile.profile, "xcolor_space", None)
    except (ImageCms.PyCMSError, OSError, ValueError) as exc:
        msg = "cannot read embedded ICC profile"
        raise ImageVariantError(msg) from exc
    if not isinstance(color_space, str) or color_space not in {"RGB ", "GRAY"}:
        msg = f"unsupported embedded ICC color space: {color_space}"
        raise ImageVariantError(msg)
    if color_space == "RGB ":
        pixels = image.convert("RGBA" if has_alpha else "RGB")
        output_mode = pixels.mode
    else:
        pixels = image.convert("L")
        output_mode = "RGB"
    try:
        converted = ImageCms.profileToProfile(
            pixels,
            input_profile,
            ImageCms.createProfile("sRGB"),
            renderingIntent=ImageCms.Intent.PERCEPTUAL,
            outputMode=output_mode,
        )
    except (ImageCms.PyCMSError, OSError, ValueError) as exc:
        msg = "cannot convert source ICC profile to sRGB"
        raise ImageVariantError(msg) from exc
    if converted is None:
        msg = "cannot convert source ICC profile to sRGB"
        raise ImageVariantError(msg)
    if has_alpha and color_space == "GRAY":
        converted.putalpha(image.convert("RGBA").getchannel("A"))
    return converted


def crop_box(width: int, height: int, override: CropBox | None) -> CropBox:
    """Use the same geometry at encoding and independently verified consumption."""
    if override is not None:
        if (
            min(override.left, override.top) < 0
            or min(override.width, override.height) <= 0
            or override.width * 3 != override.height * 4
            or override.left + override.width > width
            or override.top + override.height > height
        ):
            msg = "invalid art crop override"
            raise ImageVariantError(msg)
        return override
    left_percent, top_percent, width_percent = (
        (17, 4, 65) if width > height else (8, 14, 84)
    )
    left = (left_percent * width) // 100
    top = (top_percent * height) // 100
    k = min((width_percent * width) // 400, (width - left) // 4, (height - top) // 3)
    if k <= 0:
        msg = "source is too small for a 4:3 art crop"
        raise ImageVariantError(msg)
    return CropBox(left, top, 4 * k, 3 * k)


def _card_dimensions(width: int, height: int, size: SizeSpec) -> tuple[int, int]:
    if width > height:
        numerator, denominator = (
            (1, 1) if width <= size.max_height else (size.max_height, width)
        )
    elif width <= size.max_width and height <= size.max_height:
        numerator, denominator = 1, 1
    elif size.max_width * height <= size.max_height * width:
        numerator, denominator = size.max_width, width
    else:
        numerator, denominator = size.max_height, height
    return (
        max(1, (2 * width * numerator + denominator) // (2 * denominator)),
        max(1, (2 * height * numerator + denominator) // (2 * denominator)),
    )


def _art_dimensions(crop: CropBox, size: SizeSpec) -> tuple[int, int]:
    n = min(crop.width // 4, size.max_width // 4, size.max_height // 3)
    return (4 * n, 3 * n)


def _encode(image: Image.Image, recipe: Recipe) -> bytes:
    target = BytesIO()
    image.save(
        target,
        format="WEBP",
        lossless=False,
        quality=recipe.quality,
        alpha_quality=recipe.alpha_quality,
        method=recipe.method,
        exact=recipe.exact_alpha,
        icc_profile=b"",
        exif=b"",
        xmp=b"",
    )
    return target.getvalue()


def _blob_path(digest: str) -> str:
    if not _HASH.fullmatch(digest):
        msg = "invalid WebP blob SHA-256"
        raise ImageVariantError(msg)
    return f"images/sha256/{digest[:2]}/{digest}.webp"


def _write_blob(path: Path, data: bytes, digest: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temp_name = tempfile.mkstemp(prefix=".webp-", dir=path.parent)
    temporary = Path(temp_name)
    try:
        with os.fdopen(fd, "wb") as file:
            file.write(data)
            file.flush()
            os.fsync(file.fileno())
        try:
            os.link(temporary, path)
        except FileExistsError:
            _verify_blob(path, digest, len(data))
    finally:
        temporary.unlink(missing_ok=True)


def _verify_blob(path: Path, digest: str, byte_count: int) -> None:
    data = path.read_bytes()
    if len(data) != byte_count or hashlib.sha256(data).hexdigest() != digest:
        msg = f"content-addressed WebP blob is corrupt: {path}"
        raise ImageVariantError(msg)


def _read_cache(  # ruff: ignore[too-many-arguments] -- source geometry, recipe and shared blob inspection determine a cache hit
    path: Path,
    source: ImageSource,
    image: Image.Image,
    crop: CropBox,
    recipe: Recipe,
    blob_root: Path,
    *,
    checks: ImageChecks,
) -> VariantSet | None:
    try:
        entry = _CacheEntry.model_validate_json(path.read_bytes())
    except FileNotFoundError, ValidationError:
        return None
    if (
        entry.cache_key != path.stem
        or entry.source_sha256 != source.source_sha256
        or entry.recipe_version != recipe.version
        or entry.source_width != image.width
        or entry.source_height != image.height
    ):
        return None
    if entry.crop != list(crop.bounds) or [
        item.size_key for item in entry.variants
    ] != [size.key for size in SIZES]:
        return None
    variants = _cached_variants(
        zip(entry.variants, SIZES, strict=True),
        image,
        crop,
        source,
        recipe,
        blob_root,
        checks=checks,
    )
    if variants is None:
        return None
    return VariantSet(
        image_id=source.image_id,
        source_src_raw=source.source_src_raw,
        source_sha256=source.source_sha256,
        source_width=image.width,
        source_height=image.height,
        crop_box=crop,
        recipe_version=recipe.version,
        variants=tuple(variants),
        cache_hit=True,
    )


def _cached_variants(  # ruff: ignore[too-many-arguments] -- each cache row is bound to source geometry and the command inspection cache
    entries: Iterable[tuple[_CachedVariant, SizeSpec]],
    image: Image.Image,
    crop: CropBox,
    source: ImageSource,
    recipe: Recipe,
    blob_root: Path,
    *,
    checks: ImageChecks,
) -> list[ImageVariant] | None:
    variants: list[ImageVariant] = []
    for item, size in entries:
        if item.width <= 0 or item.height <= 0 or item.bytes <= 0:
            return None
        if size.purpose == "art":
            dimensions = _art_dimensions(crop, size)
        else:
            dimensions = _card_dimensions(image.width, image.height, size)
        if (item.width, item.height) != dimensions:
            return None
        blob = blob_root / _blob_path(item.sha256)
        if not blob.exists():
            return None
        try:
            byte_count, checksum, format_name, actual_dimensions = checks.inspect(blob)
        except OSError:
            raise ImageVariantError("Cached WebP blob is corrupt") from None
        if (byte_count, checksum, format_name, actual_dimensions) != (
            item.bytes,
            "sha256:" + item.sha256,
            "WEBP",
            dimensions,
        ):
            raise ImageVariantError("Cached WebP metadata differs from actual bytes")
        variants.append(
            ImageVariant(
                image_id=source.image_id,
                size_key=item.size_key,
                format="webp",
                path=_blob_path(item.sha256),
                width=item.width,
                height=item.height,
                bytes=item.bytes,
                sha256=item.sha256,
                recipe_version=recipe.version,
            )
        )
    return variants


def _write_cache(path: Path, result: VariantSet, key: str) -> None:
    entry = _CacheEntry(
        cache_key=key,
        source_sha256=result.source_sha256,
        recipe_version=result.recipe_version,
        source_width=result.source_width,
        source_height=result.source_height,
        crop=list(result.crop_box.bounds),
        variants=[
            _CachedVariant(
                size_key=item.size_key,
                width=item.width,
                height=item.height,
                bytes=item.bytes,
                sha256=item.sha256,
            )
            for item in result.variants
        ],
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temp_name = tempfile.mkstemp(prefix=".variant-cache-", dir=path.parent)
    temporary = Path(temp_name)
    try:
        with os.fdopen(fd, "wb") as file:
            file.write(canonical(entry.model_dump(mode="json")))
            file.flush()
            os.fsync(file.fileno())
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)
