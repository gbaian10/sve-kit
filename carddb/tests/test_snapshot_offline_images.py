"""Bilingual text, verified image closure, private bundle and preview export."""

import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb.build_inputs import InputRecord
from sve_carddb.cli import app
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.image_assets import (
    PARSERS,
    ImageBuild,
    ImageReference,
    PreviewRoots,
    build_regional_assets,
    populate_assets,
)
from sve_carddb.image_crops import load_image_crops
from sve_carddb.image_variants import ImageVariantError
from sve_carddb.products import OfficialProducts, ProductIdentities
from sve_carddb.registry.records import PrintingData
from sve_carddb.snapshot import offline, offline_images
from sve_carddb.snapshot.export import export_snapshot
from sve_carddb.snapshot.media import prepare_media
from sve_carddb.snapshot.preview import Roots, write_preview
from sve_carddb.snapshot.values import digest, object_value, parse
from sve_carddb.sources import official_en
from sve_carddb.sources.official_jp import image_url

from .image_crop_fixtures import SHARD, initialize, install, record
from .test_image_assets_en import FRONT, EnglishImages
from .test_image_assets_en import english_images as english_images  # ruff: ignore[useless-import-alias] -- reuse one sealed EN baseline
from .test_snapshot_offline import prepared as prepared  # ruff: ignore[useless-import-alias] -- reuse the existing regional text/adoption fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import SourceUse
    from sve_carddb.card_extras import CardPage
    from sve_carddb.registry.preview import PreviewPlan
    from sve_carddb.snapshot.offline import Inputs

    from .text_observation_fixtures import Case


@pytest.fixture
def regional_images(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    english_images: EnglishImages,
    image_archive_template: tuple[Path, str, str],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> tuple[Inputs, ImageBuild, PreviewRoots]:
    case, original, _ = prepared
    shutil.copytree(english_images.images.root, original.archive, dirs_exist_ok=True)
    en_source = next(
        entry.source_version_id
        for entry in english_images.images.inventory.current
        if entry.url == image_url(FRONT, official_en.card_url("synthetic"))
    )
    install(
        original.repo / "authored",
        [
            record(english_images.images.descriptor(en_source))
            | {"region": "en", "card_no": "BP02-070EN"}
        ],
    )
    revision = initialize(original.repo)
    recipe = original.model_copy(
        update={
            "revision": revision,
            "sources": tuple(
                pin.model_copy(
                    update={
                        "image_batch": english_images.images.batch_id
                        if pin.region == "en"
                        else image_archive_template[2]
                    }
                )
                for pin in original.sources
            ),
        }
    )
    identities = ProductIdentities(
        revision, digest(b"{}"), b"{}", (), {}, {}, (), case.catalog
    )
    monkeypatch.setattr(
        offline, "load_product_identities", lambda *_args, **_kwargs: identities
    )
    monkeypatch.setattr(
        offline,
        "plan_official_products",
        lambda *_args, **_kwargs: OfficialProducts(
            identities, (), (), (), (), case.identity
        ),
    )
    crops = load_image_crops(
        recipe.repo / "authored", authored_revision=recipe.revision
    )
    roots = PreviewRoots(tmp_path / "library", tmp_path / "formal", tmp_path / "cache")
    parts = tuple(
        build_regional_assets(
            FrozenSources(recipe.archive, recipe.store_id, pin.image_batch),
            roots,
            region=pin.region,
            crops=crops,
        )
        for pin in recipe.sources
    )
    assets = ImageBuild(
        tuple(item for part in parts for item in part.images),
        sum(part.elapsed_seconds for part in parts),
    )

    def references(
        _db: Database, _plan: PreviewPlan, _cards: FrozenSources, *, region: str
    ) -> tuple[ImageReference, ...]:
        src = FRONT if region == "en" else "/synthetic/0.png"
        return tuple(
            ImageReference(
                data.id,
                mapping.face_id,
                data.card_no,
                case.identity.evidence[data.region, data.card_no].source.model_copy(
                    update={"parser_version": PARSERS[data.region]}
                ),
                src,
                image_url(
                    src, case.identity.evidence[data.region, data.card_no].source.url
                ),
                data.region,
            )
            for row in case.identity.included("printing")
            if isinstance(data := row.data, PrintingData) and data.region == region
            for mapping in data.source_face_map
        )

    # The shared text fixture uses invented JSON pages; real EN HTML is covered independently.
    monkeypatch.setattr(offline_images, "plan_regional_images", references)
    return recipe, assets, roots


@pytest.mark.parametrize("format_version", ["2.0.0"])
def test_bilingual_images_bundle_snapshot_and_preview(
    format_version: str,
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
    tmp_path: Path,
) -> None:
    recipe, assets, roots = regional_images
    built = offline.build(
        recipe, images=assets, image_root=roots.preview, bundle_dir=tmp_path / "bundle"
    )
    assert {row["region"] for row in built.projection.tables["printing"]} == {
        "en",
        "jp",
    }
    assert any(row["lang"] == "en" for row in built.projection.tables["text_unit"])
    assert len(built.projection.tables["printing_image"]) == 4
    record = InputRecord.model_validate_json(built.input_content)
    assert {
        "jp_image_link",
        "en_image_link",
        "jp_image_variant",
        "en_image_variant",
    } <= {use.usage for use in record.uses}
    assert (tmp_path / "bundle/inputs.json").read_bytes() == built.input_content
    crops = load_image_crops(
        recipe.repo / "authored", authored_revision=recipe.revision
    )
    crops.verify_context(record.context)
    crop_report = object_value(
        object_value(built.report["image_assets"])["crop_overrides"]
    )
    assert crop_report["applied_source_images"] == 1
    plan = prepare_media(
        built.projection,
        roots.preview,
        revision=1,
        confirmed_images=built.confirmed_images,
    )
    snapshot = export_snapshot(
        plan.projection, built.ownership, recipe.batch(), format_version=format_version
    )
    output = Roots(tmp_path / "preview", roots.cdn)
    write_preview(
        snapshot,
        output,
        built.report,
        regions=("en", "jp"),
        image_source=roots.preview,
        media_plan=plan,
        confirmed_images=built.confirmed_images,
    )
    for path, raw in plan.blobs(roots.preview):
        assert (output.preview / path).read_bytes() == raw
    assert (output.preview / "snapshots/preview/current.json").is_file()


def test_offline_cli_reuses_both_caches_and_rejects_partial_roots(
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    recipe, _, roots = regional_images
    path = tmp_path / "recipe.json"
    path.write_text(recipe.model_dump_json())
    arguments = [
        "snapshot",
        "export-offline",
        "--inputs",
        str(path),
        "--preview-dir",
        str(tmp_path / "preview"),
        "--cdn-dir",
        str(roots.cdn),
        "--bundle-dir",
        str(tmp_path / "bundle"),
    ]
    runner = CliRunner()
    for option, root in (
        ("--image-assets-dir", roots.preview),
        ("--image-cache-dir", roots.cache),
    ):
        rejected = runner.invoke(app, [*arguments, option, str(root)])
        assert rejected.exit_code != 0
        assert "provided together" in rejected.output
        assert not (tmp_path / "preview").exists()
        assert not (tmp_path / "bundle").exists()

    def forbidden(*_args: object, **_kwargs: object) -> bytes:
        pytest.fail("CLI must reuse both complete five-size recipe caches")

    monkeypatch.setattr("sve_carddb.image_variants._encode", forbidden)
    result = runner.invoke(
        app,
        [
            *arguments,
            "--image-assets-dir",
            str(roots.preview),
            "--image-cache-dir",
            str(roots.cache),
        ],
    )
    assert result.exit_code == 0, result.stdout
    assert (tmp_path / "preview/snapshots/preview/current.json").is_file()


@pytest.mark.parametrize("region", ["en", "jp"])
def test_offline_cli_missing_cache_never_encodes_or_publishes(
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    region: str,
) -> None:
    recipe, assets, roots = regional_images
    image = next(item for item in assets.images if item.region == region)
    cache = next(
        path
        for path in (roots.cache / "image-variants").glob("*.json")
        if object_value(parse(path.read_bytes()))["source_sha256"]
        == image.result.source_sha256
    )
    cache.unlink()
    path = tmp_path / "recipe.json"
    path.write_text(recipe.model_dump_json())
    calls: list[None] = []

    def forbidden(*_args: object, **_kwargs: object) -> bytes:
        calls.append(None)
        raise AssertionError("Missing cache must not trigger image encoding")

    monkeypatch.setattr("sve_carddb.image_variants._encode", forbidden)
    result = CliRunner().invoke(
        app,
        [
            "snapshot",
            "export-offline",
            "--inputs",
            str(path),
            "--preview-dir",
            str(tmp_path / "preview"),
            "--cdn-dir",
            str(roots.cdn),
            "--bundle-dir",
            str(tmp_path / "bundle"),
            "--image-assets-dir",
            str(roots.preview),
            "--image-cache-dir",
            str(roots.cache),
        ],
    )
    assert result.exit_code != 0
    assert isinstance(result.exception, ImageVariantError)
    assert "Verified image recipe cache is incomplete" in str(result.exception)
    assert calls == []
    assert not (tmp_path / "preview").exists()
    assert not (tmp_path / "bundle").exists()
    assert not cache.exists()


@pytest.mark.parametrize(
    "failure",
    [
        "partial-batch",
        "wrong-pin",
        "missing-use",
        "old-box",
        "raw-bytes",
        "dirty-shard",
    ],
)
def test_offline_image_closure_failures_publish_nothing(
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    failure: str,
) -> None:
    recipe, assets, roots = regional_images
    if failure == "partial-batch":
        assets = replace(assets, images=assets.images[1:])
    elif failure == "wrong-pin":
        recipe = recipe.model_copy(
            update={
                "sources": tuple(
                    pin.model_copy(update={"image_batch": "sha256:" + "0" * 64})
                    if pin.region == "en"
                    else pin
                    for pin in recipe.sources
                )
            }
        )
    elif failure == "old-box":
        item = next(item for item in assets.images if item.region == "en")
        crop = item.result.crop_box
        forged = replace(
            item, result=replace(item.result, crop_box=replace(crop, top=0))
        )
        assets = replace(
            assets,
            images=tuple(forged if image is item else image for image in assets.images),
        )
    elif failure == "raw-bytes":
        item = assets.images[0]
        assets = replace(
            assets,
            images=(replace(item, raw_bytes=item.raw_bytes + 1), *assets.images[1:]),
        )
    elif failure == "dirty-shard":
        shard = recipe.repo / "authored" / SHARD
        shard.write_bytes(shard.read_bytes() + b"\n")
    else:

        def omit(
            db: Database,
            images: ImageBuild,
            refs: tuple[ImageReference, ...],
            root: Path,
        ) -> tuple[SourceUse, ...]:
            return tuple(
                use
                for use in populate_assets(db, images, refs, root)
                if use.usage != "en_image_link"
            )

        monkeypatch.setattr(offline_images, "populate_assets", omit)
    messages = {
        "partial-batch": "Offline images must cover every current regional source",
        "wrong-pin": "Offline images differ from the pinned regional image batches",
        "missing-use": "Build input use closure or context mismatch",
        "old-box": "Image crop box differs from adopted source crop",
        "raw-bytes": "Image source bytes or oriented dimensions mismatch",
        "dirty-shard": "Image crop bytes differ from pinned authored revision",
    }
    with pytest.raises(ValueError, match="^" + messages[failure] + "$"):
        offline.build(
            recipe,
            images=assets,
            image_root=roots.preview,
            bundle_dir=tmp_path / "bundle",
        )
    assert not (tmp_path / "bundle").exists()


def test_text_only_offline_does_not_depend_on_crop_data(
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
) -> None:
    recipe, _, _ = regional_images
    before = offline.build(recipe)
    (recipe.repo / "authored" / SHARD).write_bytes(b"invalid synthetic crop data")
    after = offline.build(recipe)
    assert before.input_content == after.input_content
    config = object_value(
        parse(
            InputRecord.model_validate_json(
                after.input_content
            ).context.configuration.encode()
        )
    )
    assert "image_crop_overrides" not in config
