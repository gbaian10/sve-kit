"""Synthetic encoding, cache invalidation and independently consumed crop policy."""

import shutil
from dataclasses import replace
from io import BytesIO
from typing import TYPE_CHECKING

import pytest
from PIL import Image

from sve_carddb.core.json import array, object_value
from sve_carddb.images.assets import PreviewRoots, build_regional_assets
from sve_carddb.images.crop_report import CropBinding, crop_report
from sve_carddb.images.crops import image_source_key, load_image_crops
from sve_carddb.images.variants import CropBox
from sve_carddb.ingest.archive.frozen_sources import FrozenSources

from ..support.image_crop_fixtures import install, record

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.images.assets import EncodedImage, ImageBuild
    from sve_carddb.images.crops import ImageCrops


def adopted(crops: ImageCrops, item: EncodedImage) -> bool:
    key = image_source_key(item.region, item.source.url), item.source.sha256[7:]
    return key in crops.records


@pytest.fixture(scope="module")
def crop_assets(
    empty_crops: ImageCrops,
    tmp_path_factory: pytest.TempPathFactory,
    image_archive_template: tuple[Path, str, str],
) -> tuple[
    FrozenSources, ImageCrops, ImageBuild, ImageBuild, PreviewRoots, PreviewRoots
]:
    base = tmp_path_factory.mktemp("adopted-crop-assets")
    frozen = FrozenSources(*image_archive_template)
    repo = base / "repo"
    descriptor = frozen.descriptor(frozen.inventory.current[0].source_version_id)
    row = record(descriptor)
    en = row | {
        "region": "en",
        "card_no": "TEST-EN",
        "source_key": "sha256:" + "e" * 64,
    }
    install(repo / "authored", [row, en])
    crops = load_image_crops(repo / "authored")
    default_roots = PreviewRoots(base / "default", base / "default-cache")
    override_roots = PreviewRoots(base / "override", base / "override-cache")
    default = build_regional_assets(
        frozen, default_roots, region="jp", crops=empty_crops
    )
    overridden = build_regional_assets(
        frozen, override_roots, region="jp", crops=crops, workers=2
    )
    return frozen, crops, default, overridden, default_roots, override_roots


def test_override_changes_only_art_and_keeps_orientation(
    crop_assets: tuple[
        FrozenSources, ImageCrops, ImageBuild, ImageBuild, PreviewRoots, PreviewRoots
    ],
) -> None:
    _, crops, default, overridden, _, output = crop_assets
    by_id = {item.result.image_id: item.result for item in default.images}
    for item in overridden.images:
        old = by_id[item.result.image_id]
        if adopted(crops, item):
            assert item.result.crop_box == CropBox(4, 24, 64, 48)
            assert [v.path for v in item.result.variants[:3]] == [
                v.path for v in old.variants[:3]
            ]
            assert [v.path for v in item.result.variants[3:]] != [
                v.path for v in old.variants[3:]
            ]
            art = next(v for v in item.result.variants if v.size_key == "art_m")
            with Image.open(
                BytesIO((output.preview / art.path).read_bytes())
            ) as opened:
                pixels = opened.convert("RGB")
                # The asymmetric synthetic marker moves with the crop, without a 180-degree turn.
                blue, red = pixels.getpixel((8, 10)), pixels.getpixel((30, 10))
                assert isinstance(blue, tuple)
                assert isinstance(red, tuple)
                assert blue[2] > blue[0]
                assert red[0] > red[2]
        else:
            assert item.result.variants == old.variants


def test_new_box_misses_cache_and_keeps_old_entries(
    crop_assets: tuple[
        FrozenSources, ImageCrops, ImageBuild, ImageBuild, PreviewRoots, PreviewRoots
    ],
    tmp_path: Path,
) -> None:
    frozen, crops, _, overridden, default_roots, _ = crop_assets
    stale = PreviewRoots(tmp_path / "stale", tmp_path / "stale-cache")
    shutil.copytree(default_roots.preview, stale.preview)
    shutil.copytree(default_roots.cache, stale.cache)
    before = {
        p.relative_to(tmp_path): p.read_bytes()
        for p in tmp_path.rglob("*")
        if p.is_file()
    }
    rebuilt = build_regional_assets(frozen, stale, region="jp", crops=crops)
    (chosen,) = (item for item in rebuilt.images if adopted(crops, item))
    assert [item for item in rebuilt.images if not item.result.cache_hit] == [chosen]
    assert [item.result.variants for item in rebuilt.images] == [
        item.result.variants for item in overridden.images
    ]
    after = {
        p.relative_to(tmp_path): p.read_bytes()
        for p in tmp_path.rglob("*")
        if p.is_file()
    }
    assert all(after[path] == raw for path, raw in before.items())
    art = {
        (stale.preview / v.path).relative_to(tmp_path)
        for v in chosen.result.variants
        if v.size_key.startswith("art_")
    }
    added = set(after) - set(before)
    assert art <= added
    assert len(added - art) == 1
    assert {path.parts[0] for path in added - art} == {"stale-cache"}


def test_selected_crop_must_fit_verified_oriented_source(
    crop_assets: tuple[
        FrozenSources, ImageCrops, ImageBuild, ImageBuild, PreviewRoots, PreviewRoots
    ],
    tmp_path: Path,
) -> None:
    frozen, _, _, _, _, _ = crop_assets
    descriptor = frozen.descriptor(frozen.inventory.current[0].source_version_id)
    install(tmp_path / "authored", [record(descriptor) | {"top": 1000}])
    crops = load_image_crops(tmp_path / "authored")
    output = PreviewRoots(tmp_path / "blobs", tmp_path / "cache")
    with pytest.raises(ValueError, match=r"^invalid art crop override$"):
        build_regional_assets(frozen, output, region="jp", crops=crops)


def test_report_uses_effective_owner_and_never_auto_inherits(
    crop_assets: tuple[
        FrozenSources, ImageCrops, ImageBuild, ImageBuild, PreviewRoots, PreviewRoots
    ],
) -> None:
    _, crops, _, images, _, _ = crop_assets
    chosen = next(item for item in images.images if adopted(crops, item))
    other = next(item for item in images.images if item is not chosen)
    refs = (
        CropBinding("effective", "first", "face", "jp", "TEST-001", chosen.source.url),
        CropBinding("effective", "reprint", "face", "jp", "TEST-002", other.source.url),
        CropBinding(
            "unrelated", "other-owner", "face", "jp", "TEST-003", other.source.url
        ),
        CropBinding(
            "effective", "other-face", "back", "jp", "TEST-004", other.source.url
        ),
        CropBinding(
            "effective", "shared-source", "face", "jp", "TEST-001", chosen.source.url
        ),
    )
    result = crop_report(crops, images, refs)
    assert result["applied_source_images"] == 1
    assert len(array(result["unused"])) == 1
    assert [
        object_value(item)["printing_id"]
        for item in array(result["reprint_candidates"])
    ] == ["reprint"]
    assert object_value(array(result["reprint_candidates"])[0])[
        "other_printing_ids"
    ] == ["first", "shared-source"]
    assert result["annotation_mismatches"] == []
    wrong = replace(
        crops,
        records={
            key: r.model_copy(update={"card_no": "OTHER", "region": "en"})
            for key, r in crops.records.items()
        },
    )
    reported = crop_report(wrong, images, refs)
    assert len(array(reported["annotation_mismatches"])) == 2
    assert reported["reprint_candidates"] == result["reprint_candidates"]


def test_two_adopted_printings_do_not_warn_about_each_other(
    crop_assets: tuple[
        FrozenSources, ImageCrops, ImageBuild, ImageBuild, PreviewRoots, PreviewRoots
    ],
    tmp_path: Path,
) -> None:
    frozen, _, _, images, _, _ = crop_assets
    rows = [record(frozen.descriptor(item.source.id)) for item in images.images[:2]]
    install(tmp_path / "authored", rows)
    crops = load_image_crops(tmp_path / "authored")
    refs = tuple(
        CropBinding("same-card", str(i), "front", "jp", "TEST-001", item.source.url)
        for i, item in enumerate(images.images[:2])
    )
    report = crop_report(crops, images, refs)
    assert report["applied_source_images"] == 2
    assert report["reprint_candidates"] == []


def test_other_cards_adopted_printing_does_not_affect_this_card(
    crop_assets: tuple[
        FrozenSources, ImageCrops, ImageBuild, ImageBuild, PreviewRoots, PreviewRoots
    ],
) -> None:
    _, crops, _, images, _, _ = crop_assets
    chosen = next(item for item in images.images if adopted(crops, item))
    other = next(item for item in images.images if item is not chosen)
    refs = (
        CropBinding(
            "other-card", "adopted", "front", "jp", "TEST-001", chosen.source.url
        ),
        CropBinding(
            "this-card", "unrelated", "front", "jp", "TEST-001", other.source.url
        ),
        CropBinding(
            "other-card", "other-reprint", "front", "jp", "TEST-001", other.source.url
        ),
    )
    report = crop_report(crops, images, refs)
    assert [
        (object_value(item)["printing_id"], object_value(item)["card_id"])
        for item in array(report["reprint_candidates"])
    ] == [("other-reprint", "other-card")]
