"""Synthetic encoding, cache invalidation and independently consumed crop policy."""

import shutil
from dataclasses import replace
from io import BytesIO
from typing import TYPE_CHECKING

import pytest
from PIL import Image

import sve_carddb.image_crop_report as report_module
from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.image_assets import (
    ImageReference,
    PreviewRoots,
    build_regional_assets,
    verify_asset_sources,
)
from sve_carddb.image_crop_report import crop_report
from sve_carddb.image_crops import image_source_key, load_image_crops
from sve_carddb.image_variants import CropBox
from sve_carddb.snapshot.values import array, object_value

from .image_crop_fixtures import install, record

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.image_assets import EncodedImage, ImageBuild
    from sve_carddb.image_crops import ImageCrops


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
    default_roots = PreviewRoots(base / "default", base / "cdn", base / "default-cache")
    override_roots = PreviewRoots(
        base / "override", base / "cdn", base / "override-cache"
    )
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
    frozen, crops, default, overridden, _, output = crop_assets
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
    verify_asset_sources(overridden, {frozen.store_id: frozen.root}, crops=crops)


def test_consumer_rejects_old_default_and_forged_box_metadata(
    crop_assets: tuple[
        FrozenSources, ImageCrops, ImageBuild, ImageBuild, PreviewRoots, PreviewRoots
    ],
) -> None:
    frozen, crops, default, overridden, _, _ = crop_assets
    stores = {frozen.store_id: frozen.root}
    with pytest.raises(
        ValueError, match=r"^Image crop box differs from adopted source crop$"
    ):
        verify_asset_sources(default, stores, crops=crops)
    with pytest.raises(
        ValueError, match=r"^Image crop box differs from adopted source crop$"
    ):
        verify_asset_sources(overridden, stores)
    forged = replace(
        overridden,
        images=tuple(
            replace(item, result=replace(item.result, crop_box=CropBox(4, 25, 64, 48)))
            if item.result.crop_box == CropBox(4, 24, 64, 48)
            else item
            for item in overridden.images
        ),
    )
    with pytest.raises(
        ValueError, match=r"^Image crop box differs from adopted source crop$"
    ):
        verify_asset_sources(forged, stores, crops=crops)


def test_reuse_requires_new_box_cache_and_never_repairs(
    crop_assets: tuple[
        FrozenSources, ImageCrops, ImageBuild, ImageBuild, PreviewRoots, PreviewRoots
    ],
    tmp_path: Path,
) -> None:
    frozen, crops, _, overridden, default_roots, override_roots = crop_assets
    missing = PreviewRoots(
        tmp_path / "missing", tmp_path / "cdn", tmp_path / "missing-cache"
    )
    shutil.copytree(default_roots.preview, missing.preview)
    shutil.copytree(default_roots.cache, missing.cache)
    before = {
        p.relative_to(tmp_path): p.read_bytes()
        for p in tmp_path.rglob("*")
        if p.is_file()
    }
    with pytest.raises(
        ValueError, match=r"^Verified image recipe cache is incomplete$"
    ):
        build_regional_assets(
            frozen, missing, region="jp", crops=crops, reuse_only=True
        )
    assert before == {
        p.relative_to(tmp_path): p.read_bytes()
        for p in tmp_path.rglob("*")
        if p.is_file()
    }
    reused = build_regional_assets(
        frozen, override_roots, region="jp", crops=crops, reuse_only=True
    )
    assert all(item.result.cache_hit for item in reused.images)
    assert [item.result.variants for item in reused.images] == [
        item.result.variants for item in overridden.images
    ]


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
    output = PreviewRoots(tmp_path / "blobs", tmp_path / "cdn", tmp_path / "cache")
    with pytest.raises(ValueError, match=r"^invalid art crop override$"):
        build_regional_assets(frozen, output, region="jp", crops=crops)


def test_report_uses_effective_owner_and_never_auto_inherits(
    crop_assets: tuple[
        FrozenSources, ImageCrops, ImageBuild, ImageBuild, PreviewRoots, PreviewRoots
    ],
    monkeypatch: pytest.MonkeyPatch,
) -> None:

    _, crops, _, images, _, _ = crop_assets
    chosen = next(item for item in images.images if adopted(crops, item))
    other = next(item for item in images.images if item is not chosen)
    refs = (
        ImageReference(
            "first",
            "face",
            "TEST-001",
            chosen.source,
            chosen.source.url,
            chosen.source.url,
            "jp",
        ),
        ImageReference(
            "reprint",
            "face",
            "TEST-002",
            other.source,
            other.source.url,
            other.source.url,
            "jp",
        ),
        ImageReference(
            "other-owner",
            "face",
            "TEST-003",
            other.source,
            other.source.url,
            other.source.url,
            "jp",
        ),
        ImageReference(
            "other-face",
            "back",
            "TEST-004",
            other.source,
            other.source.url,
            other.source.url,
            "jp",
        ),
    )

    class Owners:
        def rows(self, *_args: object) -> list[dict[str, str]]:
            return [
                {"id": "first", "card_id": "effective"},
                {"id": "reprint", "card_id": "effective"},
                {"id": "other-owner", "card_id": "unrelated"},
                {"id": "other-face", "card_id": "effective"},
            ]

    monkeypatch.setattr(report_module, "Source", lambda _db: Owners())
    with create_database(compile_build(("images",))) as db:
        result = crop_report(crops, images, refs, db)
    assert result["applied_source_images"] == 1
    assert len(array(result["unused"])) == 1
    assert [
        object_value(item)["printing_id"]
        for item in array(result["reprint_candidates"])
    ] == ["reprint"]
    assert result["annotation_mismatches"] == []
    # Labels do not govern selection or effective ownership.
    wrong = replace(
        crops,
        records={
            key: r.model_copy(update={"card_no": "OTHER", "region": "en"})
            for key, r in crops.records.items()
        },
    )
    with create_database(compile_build(("images",))) as db:
        reported = crop_report(wrong, images, refs, db)
    assert len(array(reported["annotation_mismatches"])) == 1
    assert reported["reprint_candidates"] == result["reprint_candidates"]


def _diagnostics(
    crops: ImageCrops,
    images: ImageBuild,
    bindings: tuple[tuple[int, str, str], ...],
    monkeypatch: pytest.MonkeyPatch,
) -> dict[str, object]:
    refs = tuple(
        ImageReference(
            printing,
            "front",
            "TEST-001",
            images.images[index].source,
            images.images[index].source.url,
            images.images[index].source.url,
            "jp",
        )
        for index, printing, _card in bindings
    )

    class Owners:
        def rows(self, *_args: object) -> list[dict[str, str]]:
            return [
                {"id": printing, "card_id": card} for _index, printing, card in bindings
            ]

    monkeypatch.setattr(report_module, "Source", lambda _db: Owners())
    with create_database(compile_build(("images",))) as db:
        return dict(crop_report(crops, images, refs, db))


def test_two_adopted_printings_do_not_warn_about_each_other(
    crop_assets: tuple[
        FrozenSources, ImageCrops, ImageBuild, ImageBuild, PreviewRoots, PreviewRoots
    ],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    frozen, _, _, images, _, _ = crop_assets
    rows = [record(frozen.descriptor(item.source.id)) for item in images.images[:2]]
    install(tmp_path / "authored", rows)
    crops = load_image_crops(tmp_path / "authored")
    report = _diagnostics(
        crops,
        images,
        ((0, "first", "same-card"), (1, "reprint", "same-card")),
        monkeypatch,
    )
    assert report["applied_source_images"] == 2
    assert report["reprint_candidates"] == []


def test_other_cards_adopted_printing_does_not_affect_this_card(
    crop_assets: tuple[
        FrozenSources, ImageCrops, ImageBuild, ImageBuild, PreviewRoots, PreviewRoots
    ],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _, crops, _, images, _, _ = crop_assets
    selected = next(
        index for index, item in enumerate(images.images) if adopted(crops, item)
    )
    plain = next(index for index, item in enumerate(images.images) if index != selected)
    report = _diagnostics(
        crops,
        images,
        (
            (selected, "adopted", "other-card"),
            (plain, "unrelated", "this-card"),
            (plain, "other-reprint", "other-card"),
        ),
        monkeypatch,
    )
    candidates = report["reprint_candidates"]
    assert isinstance(candidates, list)
    assert [
        (object_value(item)["printing_id"], object_value(item)["card_id"])
        for item in candidates
    ] == [("other-reprint", "other-card")]
