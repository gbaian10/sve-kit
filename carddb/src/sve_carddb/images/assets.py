"""Convert sealed regional image batches into verified content-addressed variants."""

from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from time import perf_counter
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, digest
from sve_carddb.images.checks import ImageChecks
from sve_carddb.images.variants import (
    DEFAULT_RECIPE,
    SIZES,
    ImageSource,
    VariantSet,
    build_variants,
)
from sve_carddb.ingest.archive.store import resolve_within

if TYPE_CHECKING:
    from collections.abc import Iterable

    from sve_carddb.core.provenance import Source
    from sve_carddb.core.regions import Region
    from sve_carddb.images.crops import ImageCrops
    from sve_carddb.ingest.archive.frozen_sources import FrozenSources

MAX_WORKERS = 4


def conversion_image_id(source_version_id: str) -> str:
    """Keep conversion IDs distinct from the later HTML binding IDs."""
    return "img:v1:" + digest(canonical({"source_id": source_version_id}))[7:]


@dataclass(frozen=True)
class PreviewRoots:
    preview: Path
    cache: Path

    def validate(self, protected: Iterable[Path]) -> None:
        """Resolve aliases before rejecting output, cache and protected input overlap."""
        roots = (self.preview, self.cache, *protected)
        if any(not root.is_absolute() for root in roots):
            raise ValueError("Image roots must be absolute paths")
        if any(root.is_symlink() for root in roots):
            raise ValueError("Image roots must not be symlinks")
        resolved = tuple(root.resolve() for root in roots)
        for index, root in enumerate(resolved[:2]):
            if any(
                root.is_relative_to(other) or other.is_relative_to(root)
                for other in resolved[index + 1 :]
            ):
                raise ValueError("Image output, cache and input roots must not overlap")


@dataclass(frozen=True)
class EncodedImage:
    source: Source
    raw_bytes: int
    result: VariantSet
    region: Region
    elapsed_seconds: float


@dataclass(frozen=True)
class ImageBuild:
    images: tuple[EncodedImage, ...]
    elapsed_seconds: float

    def execution(self) -> dict[str, int]:
        """Measure this run only; per-image times overlap when workers run in parallel."""
        hits = [item for item in self.images if item.result.cache_hit]
        encoded = [item for item in self.images if not item.result.cache_hit]
        return {
            "wall_milliseconds": round(self.elapsed_seconds * 1000),
            "cache_hits": len(hits),
            "new_encodings": len(encoded),
            "reuse_milliseconds": round(sum(i.elapsed_seconds for i in hits) * 1000),
            "new_encoding_milliseconds": round(
                sum(i.elapsed_seconds for i in encoded) * 1000
            ),
        }


def build_regional_assets(
    images: FrozenSources,
    roots: PreviewRoots,
    *,
    region: Region,
    crops: ImageCrops,
    workers: int = 1,
    checks: ImageChecks | None = None,
) -> ImageBuild:
    """Convert one pinned regional image batch, reusing every verified cache hit."""
    if region not in {"jp", "en"}:
        raise ValueError("Unsupported image region")
    roots.validate((images.root,))
    if type(workers) is not int or not 1 <= workers <= MAX_WORKERS:
        raise ValueError("Image worker count must be between 1 and 4")
    if {(scope.provider, scope.kind) for scope in images.inventory.scope} != {
        (region, "image")
    }:
        raise ValueError(
            f"Image conversion requires an exclusively {region.upper()} image batch"
        )
    checks = checks or ImageChecks()
    start = perf_counter()

    def convert(version: str) -> EncodedImage:
        began = perf_counter()
        source, descriptor = images.metadata(
            version, parser_version=DEFAULT_RECIPE.version
        )
        raw = resolve_within(
            images.root, PurePosixPath(images.entries[version].blob.path)
        )
        if (descriptor.provider, descriptor.kind, source.kind) != (
            region,
            "image",
            "image",
        ):
            raise ValueError(
                f"{region.upper()} image batch contains another provider or source kind"
            )
        source_bytes: bytes | Path = raw
        if not checks.reuse:
            source, source_bytes, descriptor = images.read(
                version, parser_version=DEFAULT_RECIPE.version
            )
        # HTML src is supplied later by the source-bound text observations.
        result = build_variants(
            ImageSource(
                conversion_image_id(source.id),
                source_bytes,
                source.sha256.removeprefix("sha256:"),
                source.url,
                "sve_card",
                "approved",
                "available",
            ),
            blob_root=roots.preview,
            cache_root=roots.cache,
            override=crops.box(descriptor),
            checks=checks,
        )
        return EncodedImage(
            source, descriptor.raw_bytes, result, region, perf_counter() - began
        )

    versions = tuple(
        sorted(item.source_version_id for item in images.inventory.current)
    )
    if workers == 1:
        results = tuple(map(convert, versions))
    else:
        with ThreadPoolExecutor(max_workers=workers) as executor:
            results = tuple(executor.map(convert, versions, buffersize=workers))
    result = ImageBuild(results, perf_counter() - start)
    verify_assets(result, roots.preview, checks=checks)
    return result


def verify_assets(
    build: ImageBuild, preview: Path, *, checks: ImageChecks | None = None
) -> None:
    """Recheck five-size closure, provenance and actual decoded blob metadata."""
    checks = checks or ImageChecks()
    ids = [item.result.image_id for item in build.images]
    urls = [item.source.url for item in build.images]
    if len(set(ids)) != len(ids) or len(set(urls)) != len(urls):
        raise ValueError("Image asset identities must be unique")
    for item in build.images:
        result = item.result
        if (
            result.image_id != conversion_image_id(item.source.id)
            or item.source.kind != "image"
            or item.source.parser_version != DEFAULT_RECIPE.version
            or result.recipe_version != DEFAULT_RECIPE.version
            or [v.size_key for v in result.variants] != [size.key for size in SIZES]
        ):
            raise ValueError("Image source, recipe or five-size closure mismatch")
        if (result.source_src_raw, "sha256:" + result.source_sha256) != (
            item.source.url,
            item.source.sha256,
        ) or min(item.raw_bytes, result.source_width, result.source_height) <= 0:
            raise ValueError("Image source metadata mismatch")
        for variant in result.variants:
            if (
                variant.image_id != result.image_id
                or not _public_variant(variant.format, variant.is_original)
                or variant.recipe_version != result.recipe_version
            ):
                raise ValueError("Image variant identity or recipe mismatch")
            expected_path = f"images/sha256/{variant.sha256[:2]}/{variant.sha256}.webp"
            if variant.path != expected_path:
                raise ValueError("Image variant path is not content addressed")
            if checks.reuse and result.cache_hit:
                # The cache read already matched existence and size; --verify-all rereads bytes.
                continue
            metadata = checks.inspect(
                resolve_within(preview, PurePosixPath(variant.path))
            )
            if metadata != (
                variant.bytes,
                "sha256:" + variant.sha256,
                "WEBP",
                (variant.width, variant.height),
            ):
                raise ValueError(
                    "Image variant blob hash, bytes, dimensions or format mismatch"
                )


def _public_variant(format_name: str, is_original: bool) -> bool:
    return format_name == "webp" and not is_original
