"""Synthetic archive images, independent bindings and interrupted asset publication."""

import hashlib
import shutil
from dataclasses import replace
from itertools import count
from typing import TYPE_CHECKING

import pytest

from sve_carddb import image_assets, image_variants
from sve_carddb.core.json import object_value, parse
from sve_carddb.core.regions import SourceRegion
from sve_carddb.image_assets import (
    PreviewRoots,
    build_regional_assets,
    verify_asset_sources,
    verify_assets,
)
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind, Manifest
from sve_carddb.ingest.archive.source_archive import ArchiveError, seal_batch

from .test_image_variants import png
from .test_registry_preview_archive import RAW
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from sve_carddb.image_assets import ImageBuild
    from sve_carddb.image_crops import ImageCrops
    from sve_carddb.image_variants import VariantSet


@pytest.fixture
def frozen(
    tmp_path: Path, image_archive_template: tuple[Path, str, str]
) -> FrozenSources:
    root, store_id, batch = image_archive_template
    destination = tmp_path / "input/archive"
    # Staging tests append pages, so copy the synthetic latest data and manifest too.
    shutil.copytree(root.parent, destination.parent)
    return FrozenSources(destination, store_id, batch)


def roots(tmp_path: Path) -> PreviewRoots:
    return PreviewRoots(tmp_path / "preview", tmp_path / "cache")


def bytes_by_path(build: ImageBuild, root: Path) -> dict[str, bytes]:
    return {
        variant.path: (root / variant.path).read_bytes()
        for item in build.images
        for variant in item.result.variants
    }


def test_serial_parallel_resume_and_hashes_use_only_frozen_bytes(
    empty_crops: ImageCrops,
    tmp_path: Path,
    frozen: FrozenSources,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    def forbidden(*_args: object, **_kwargs: object) -> None:
        pytest.fail("live manifest access")

    monkeypatch.setattr(Manifest, "open", forbidden)
    monkeypatch.setattr(Manifest, "open_live", forbidden)
    before = {p: p.read_bytes() for p in frozen.root.rglob("*") if p.is_file()}
    serial_roots = roots(tmp_path / "serial")
    parallel_roots = roots(tmp_path / "parallel")
    serial = build_regional_assets(
        frozen, serial_roots, workers=1, region="jp", crops=empty_crops
    )
    parallel = build_regional_assets(
        frozen, parallel_roots, workers=4, region="jp", crops=empty_crops
    )
    assert [item.result.variants for item in serial.images] == [
        item.result.variants for item in parallel.images
    ]
    assert bytes_by_path(serial, serial_roots.preview) == bytes_by_path(
        parallel, parallel_roots.preview
    )
    assert len(serial.images) == 3
    assert all(len(item.result.variants) == 5 for item in serial.images)
    assert serial.images[0].result.variants[0].format == "webp"
    report = serial.report()
    blobs = bytes_by_path(serial, serial_roots.preview)
    assert report["unique_webp_files"] == len(blobs)
    assert report["unique_webp_bytes"] == sum(map(len, blobs.values()))
    assert report["converted_variant_rows"] == 15
    for path, data in blobs.items():
        assert (
            path
            == f"images/sha256/{hashlib.sha256(data).hexdigest()[:2]}/{hashlib.sha256(data).hexdigest()}.webp"
        )
    assert not (serial_roots.preview / "snapshots").exists()
    verify_asset_sources(serial, {frozen.store_id: frozen.root})
    assert before == {p: p.read_bytes() for p in frozen.root.rglob("*") if p.is_file()}

    def unexpected_encode(*_args: object, **_kwargs: object) -> bytes:
        pytest.fail("resume must reuse validated image cache")

    monkeypatch.setattr(image_variants, "_encode", unexpected_encode)
    resumed = build_regional_assets(
        frozen, serial_roots, workers=2, region="jp", crops=empty_crops
    )
    assert resumed.execution()["cache_hits"] == 3
    assert bytes_by_path(resumed, serial_roots.preview) == blobs


@pytest.mark.parametrize("workers", [0, -1, 5, True])
def test_worker_limit_is_checked_before_writing(
    empty_crops: ImageCrops, tmp_path: Path, frozen: FrozenSources, workers: int
) -> None:
    output = roots(tmp_path)
    with pytest.raises(ValueError, match="between 1 and 4"):
        build_regional_assets(
            frozen, output, workers=workers, region="jp", crops=empty_crops
        )
    assert not output.preview.exists()


def test_build_rechecks_blob_tampering_before_returning(
    empty_crops: ImageCrops,
    tmp_path: Path,
    frozen: FrozenSources,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    output = roots(tmp_path)
    original: Callable[..., VariantSet] = image_variants.build_variants
    calls = 0

    def corrupt_after_encoding(*args: object, **kwargs: object) -> VariantSet:
        nonlocal calls
        calls += 1
        result = original(*args, **kwargs)
        if calls == len(frozen.inventory.current):
            path = output.preview / result.variants[0].path
            data = path.read_bytes()
            path.write_bytes(data[:-1] + bytes([data[-1] ^ 1]))
        return result

    monkeypatch.setattr(image_assets, "build_variants", corrupt_after_encoding)
    with pytest.raises(
        ValueError, match="blob hash, bytes, dimensions or format mismatch"
    ):
        build_regional_assets(frozen, output, region="jp", crops=empty_crops)


@pytest.mark.parametrize(
    "case",
    [
        "same",
        "preview-in-cache",
        "cache-in-preview",
        "archive",
        "symlink",
        "nested-symlink",
        "relative",
    ],
)
def test_each_root_isolation_constraint_fails_before_writing(
    empty_crops: ImageCrops,
    tmp_path: Path,
    frozen: FrozenSources,
    case: str,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.chdir(tmp_path)
    output = roots(tmp_path)
    if case == "same":
        output = replace(output, preview=output.cache)
    elif case == "preview-in-cache":
        output = replace(output, preview=output.cache / "nested")
    elif case == "cache-in-preview":
        output = replace(output, cache=output.preview / "private")
    elif case == "archive":
        output = replace(output, preview=frozen.root / "unsafe")
    elif case == "relative":
        output = replace(output, preview=type(tmp_path)("relative"))
    elif case == "symlink":
        (tmp_path / "independent-target").mkdir()
        output.preview.symlink_to(
            tmp_path / "independent-target", target_is_directory=True
        )
    else:
        output.preview.mkdir()
        (output.preview / "images").symlink_to(frozen.root, target_is_directory=True)
    with pytest.raises(ValueError, match=r"overlap|symlink|absolute"):
        build_regional_assets(frozen, output, region="jp", crops=empty_crops)
    assert not list(frozen.root.rglob("*.webp"))
    assert not output.cache.exists()


@pytest.mark.parametrize("case", ["en", "mixed", "card"])
def test_wrong_batch_provider_or_kind_is_rejected(
    empty_crops: ImageCrops, tmp_path: Path, case: str
) -> None:
    store = _store(tmp_path / "input")
    data = png(80, 112)
    resource = _resource("https://example.invalid/image.png", "raw/image.png", data)
    if case == "en":
        resource = replace(resource, region=SourceRegion.EN)
    elif case == "card":
        resource = replace(resource, kind=Kind.CARD, content_type="text/html")
    _put(store, resource, data)
    if case == "mixed":
        _put(
            store,
            _resource("https://example.invalid/card", "raw/card.html", RAW, Kind.CARD),
            RAW,
        )
    batch = seal_batch(store)
    frozen = FrozenSources(store.root, store.store_id, batch.batch_id)
    with pytest.raises(ValueError, match="exclusively JP image batch"):
        build_regional_assets(frozen, roots(tmp_path), region="jp", crops=empty_crops)
    assert not roots(tmp_path).preview.exists()


@pytest.mark.parametrize(
    "case",
    [
        "missing-variant",
        "extra-variant",
        "duplicate-image",
        "source-hash",
        "source-url",
        "recipe",
        "recipe-all",
        "variant-id",
        "format",
        "original",
        "variant-recipe",
        "path",
        "bytes",
        "dimensions",
        "missing-blob",
        "corrupt-blob",
    ],
)
def test_independent_asset_tampering_is_rejected(  # ruff: ignore[complex-structure, too-many-branches] -- independent invariant counterexamples share one verified fixture
    empty_crops: ImageCrops, tmp_path: Path, frozen: FrozenSources, case: str
) -> None:
    output = roots(tmp_path)
    build = build_regional_assets(frozen, output, region="jp", crops=empty_crops)
    item = build.images[0]
    result = item.result
    variant = result.variants[0]
    if case == "missing-variant":
        result = replace(result, variants=result.variants[:-1])
    elif case == "extra-variant":
        result = replace(result, variants=(*result.variants, variant))
    elif case == "duplicate-image":
        build = replace(build, images=(*build.images, item))
    elif case == "source-hash":
        result = replace(result, source_sha256="0" * 64)
    elif case == "source-url":
        result = replace(result, source_src_raw="https://example.invalid/wrong")
    elif case == "recipe":
        result = replace(result, recipe_version="wrong")
    elif case == "recipe-all":
        result = replace(
            result,
            recipe_version="wrong",
            variants=tuple(replace(v, recipe_version="wrong") for v in result.variants),
        )
    elif case in {"format", "original"}:
        object.__setattr__(  # ruff: ignore[unnecessary-dunder-call] -- forge immutable metadata at the runtime boundary
            variant,
            "format" if case == "format" else "is_original",
            "png" if case == "format" else True,
        )
    elif case in {"missing-blob", "corrupt-blob"}:
        path = output.preview / variant.path
        if case == "missing-blob":
            path.unlink()
        else:
            data = path.read_bytes()
            path.write_bytes(data[:-1] + bytes([data[-1] ^ 1]))
    else:
        changes = {
            "variant-id": replace(variant, image_id="wrong"),
            "variant-recipe": replace(variant, recipe_version="wrong"),
            "path": replace(variant, path="images/wrong.webp"),
            "bytes": replace(variant, bytes=variant.bytes + 1),
            "dimensions": replace(variant, width=variant.width + 1),
        }
        if case == "path":
            (output.preview / "images/wrong.webp").write_bytes(
                (output.preview / variant.path).read_bytes()
            )
        result = replace(result, variants=(changes[case], *result.variants[1:]))
    if case != "duplicate-image":
        build = replace(build, images=(replace(item, result=result), *build.images[1:]))
    with pytest.raises((ValueError, FileNotFoundError)):
        verify_assets(build, output.preview)


@pytest.mark.parametrize("case", ["raw-bytes", "source-width", "unconfigured"])
def test_source_dimensions_and_raw_bytes_are_checked_independently(
    empty_crops: ImageCrops, tmp_path: Path, frozen: FrozenSources, case: str
) -> None:
    build = build_regional_assets(
        frozen, roots(tmp_path), region="jp", crops=empty_crops
    )
    item = build.images[0]
    if case == "raw-bytes":
        item = replace(item, raw_bytes=item.raw_bytes + 1)
    elif case == "source-width":
        item = replace(
            item, result=replace(item.result, source_width=item.result.source_width + 1)
        )
    build = replace(build, images=(item, *build.images[1:]))
    with pytest.raises(
        ValueError, match=r"source bytes|not configured|crop box differs"
    ):
        verify_asset_sources(
            build, {} if case == "unconfigured" else {frozen.store_id: frozen.root}
        )


def test_corrupt_archive_is_never_replaced_by_latest(
    empty_crops: ImageCrops, tmp_path: Path, frozen: FrozenSources
) -> None:
    raw = frozen.root / frozen.inventory.entries[0].blob.path
    raw.write_bytes(b"tampered")
    with pytest.raises(ArchiveError, match="hash"):
        build_regional_assets(frozen, roots(tmp_path), region="jp", crops=empty_crops)


def test_interrupted_conversion_resumes_complete_blobs(
    empty_crops: ImageCrops,
    tmp_path: Path,
    frozen: FrozenSources,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    output = roots(tmp_path)
    original: Callable[..., image_variants.VariantSet] = image_variants.build_variants
    calls = 0

    def interrupted(*args: object, **kwargs: object) -> image_variants.VariantSet:
        nonlocal calls
        calls += 1
        if calls == 2:
            raise OSError("synthetic disk full")
        return original(*args, **kwargs)

    monkeypatch.setattr(image_assets, "build_variants", interrupted)
    with pytest.raises(OSError, match="disk full"):
        build_regional_assets(frozen, output, workers=1, region="jp", crops=empty_crops)
    before = {p: p.read_bytes() for p in output.preview.rglob("*.webp")}
    assert before
    assert not (output.preview / "snapshots").exists()
    monkeypatch.setattr(image_assets, "build_variants", original)
    resumed = build_regional_assets(frozen, output, region="jp", crops=empty_crops)
    assert resumed.execution()["cache_hits"] >= 1
    assert all(p.read_bytes() == data for p, data in before.items())
    assert len(resumed.images) == 3


@pytest.fixture(scope="module")
def cached_images(
    empty_crops: ImageCrops,
    tmp_path_factory: pytest.TempPathFactory,
    image_archive_template: tuple[Path, str, str],
) -> tuple[FrozenSources, PreviewRoots, ImageBuild]:
    root, store_id, batch = image_archive_template
    output = roots(tmp_path_factory.mktemp("cached-images"))
    source = FrozenSources(root, store_id, batch)
    build = build_regional_assets(source, output, region="jp", crops=empty_crops)
    return source, output, build


def tree(root: Path) -> dict[Path, bytes]:
    return {p.relative_to(root): p.read_bytes() for p in root.rglob("*") if p.is_file()}


@pytest.mark.parametrize("case", ["hit", "no-cache", "bad-cache", "no-blob"])
def test_cache_hits_are_reused_and_misses_are_encoded(
    empty_crops: ImageCrops,
    cached_images: tuple[FrozenSources, PreviewRoots, ImageBuild],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    case: str,
) -> None:
    frozen, base, first = cached_images
    output = roots(tmp_path)
    shutil.copytree(base.preview, output.preview)
    shutil.copytree(base.cache, output.cache)
    stale: set[str] = set()
    if case in {"no-cache", "bad-cache"}:
        entry = next(output.cache.rglob("*.json"))
        sha = object_value(parse(entry.read_bytes()))["source_sha256"]
        stale = {
            i.result.image_id for i in first.images if i.result.source_sha256 == sha
        }
        if case == "no-cache":
            entry.unlink()
        else:
            entry.write_bytes(b"{}")
    elif case == "no-blob":
        blob = next(output.preview.rglob("*.webp"))
        path = blob.relative_to(output.preview).as_posix()
        stale = {
            item.result.image_id
            for item in first.images
            if any(variant.path == path for variant in item.result.variants)
        }
        blob.unlink()
    else:

        def forbidden(*_args: object, **_kwargs: object) -> None:
            pytest.fail("a verified cache hit cannot encode or write")

        for name in ("_encode", "_write_blob", "_write_cache"):
            monkeypatch.setattr(image_variants, name, forbidden)
    archive = tree(frozen.root)
    build = build_regional_assets(
        frozen, output, workers=4, region="jp", crops=empty_crops
    )
    # Identical sources share one cache key, so parallel workers may encode either copy.
    misses = {i.result.image_id for i in build.images if not i.result.cache_hit}
    assert misses <= stale
    assert bool(misses) == bool(stale)
    assert build.execution()["new_encodings"] == len(misses)
    assert build.execution()["cache_hits"] == len(build.images) - len(misses)
    assert [item.result.variants for item in build.images] == [
        item.result.variants for item in first.images
    ]
    assert tree(output.preview) == tree(base.preview)
    assert tree(output.cache) == tree(base.cache)
    assert tree(frozen.root) == archive


def test_execution_splits_reuse_and_encoding_time(
    empty_crops: ImageCrops,
    cached_images: tuple[FrozenSources, PreviewRoots, ImageBuild],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    frozen, base, _ = cached_images
    output = roots(tmp_path)
    shutil.copytree(base.preview, output.preview)
    shutil.copytree(base.cache, output.cache)
    next(output.cache.rglob("*.json")).unlink()
    ticks = count()
    # Each clock read advances one second, so every image spans exactly one second.
    monkeypatch.setattr(image_assets, "perf_counter", lambda: float(next(ticks)))
    build = build_regional_assets(
        frozen, output, workers=1, region="jp", crops=empty_crops
    )
    assert build.execution() == {
        "wall_milliseconds": (2 * len(build.images) + 1) * 1000,
        "cache_hits": len(build.images) - 1,
        "new_encodings": 1,
        "reuse_milliseconds": (len(build.images) - 1) * 1000,
        "new_encoding_milliseconds": 1000,
    }
