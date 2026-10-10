"""Bilingual text, verified image closure, private bundle and preview export."""

import shutil
from itertools import count
from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.core.json import object_value, parse
from sve_carddb.core.provenance import InputRecord
from sve_carddb.domains.products import OfficialProducts, ProductIdentities
from sve_carddb.domains.registry.records import PrintingData
from sve_carddb.export.media import prepare_media
from sve_carddb.export.preview import Roots, write_preview
from sve_carddb.export.transport import export_snapshot
from sve_carddb.images.assets import (
    PARSERS,
    ImageBuild,
    ImageReference,
    PreviewRoots,
    build_regional_assets,
)
from sve_carddb.images.crops import FILE, load_image_crops
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.parse.pages import official_en
from sve_carddb.parse.pages.official_jp import image_url
from sve_carddb.workflows import export as commands
from sve_carddb.workflows import offline, offline_images

from ..images.test_image_assets_en import FRONT, EnglishImages
from ..images.test_image_assets_en import english_images as english_images  # ruff: ignore[useless-import-alias] -- reuse one sealed EN baseline
from ..support.image_crop_fixtures import install, record
from .test_snapshot_offline import prepared as prepared  # ruff: ignore[useless-import-alias] -- reuse the existing regional text/adoption fixture

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.build import Database
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.card_extras import CardPage
    from sve_carddb.domains.registry.preview import PreviewPlan
    from sve_carddb.images.checks import ImageChecks
    from sve_carddb.images.crops import ImageCrops
    from sve_carddb.workflows.offline import Inputs

    from ..support.text_observation_fixtures import Case


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
    recipe = original.model_copy(
        update={
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
    identities = ProductIdentities(recipe.revision, (), {}, {}, (), case.catalog)
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
    crops = load_image_crops(recipe.repo / "authored")
    roots = PreviewRoots(tmp_path / "library", tmp_path / "cache")
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


@pytest.mark.parametrize("format_version", ["3.0.0"])
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
    crop_report = object_value(
        object_value(built.report["image_assets"])["crop_overrides"]
    )
    assert crop_report["applied_source_images"] == 1
    plan = prepare_media(built.projection, roots.preview, revision=1)
    snapshot = export_snapshot(
        plan.projection, built.ownership, recipe.batch(), format_version=format_version
    )
    output = Roots(tmp_path / "preview", tmp_path / "private")
    write_preview(
        snapshot,
        output,
        built.report,
        regions=("en", "jp"),
        image_source=roots.preview,
        media_plan=plan,
    )
    for path, raw in plan.blobs(roots.preview):
        assert (output.preview / path).read_bytes() == raw
    assert (output.preview / "snapshots/preview/current.json").is_file()


def offline_arguments(recipe: Inputs, tmp_path: Path) -> list[str]:
    path = tmp_path / "recipe.json"
    path.write_text(recipe.model_dump_json())
    return [
        "snapshot",
        "export-offline",
        "--inputs",
        str(path),
        "--preview-dir",
        str(tmp_path / "preview"),
        "--private-dir",
        str(tmp_path / "private"),
        "--bundle-dir",
        str(tmp_path / "bundle"),
    ]


def image_execution(stdout: str) -> dict[str, JsonValue]:
    return object_value(object_value(parse(stdout.encode()))["image_execution"])


def test_offline_cli_reuses_both_caches_and_rejects_partial_roots(
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    recipe, assets, roots = regional_images
    arguments = offline_arguments(recipe, tmp_path)
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

    seen: list[int] = []

    def recorded(
        images: FrozenSources,
        output: PreviewRoots,
        *,
        region: Region,
        crops: ImageCrops,
        workers: int = 1,
        checks: ImageChecks | None = None,
    ) -> ImageBuild:
        seen.append(workers)
        return build_regional_assets(
            images, output, region=region, crops=crops, workers=workers, checks=checks
        )

    monkeypatch.setattr("sve_carddb.images.variants._encode", forbidden)
    monkeypatch.setattr(commands, "build_regional_assets", recorded)
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
    assert seen == [2, 2]
    execution = image_execution(result.stdout)
    assert execution["cache_hits"] == len(assets.images)
    assert execution["new_encodings"] == 0
    assert execution["new_encoding_milliseconds"] == 0
    assert execution["workers"] == 2


@pytest.mark.parametrize("region", ["en", "jp"])
def test_offline_cli_encodes_missing_cache_and_reports_time(
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
    cached = cache.read_bytes()
    cache.unlink()
    ticks = count()
    # Each clock read advances one second, so every image spans exactly one second.
    monkeypatch.setattr(
        "sve_carddb.images.assets.perf_counter", lambda: float(next(ticks))
    )
    result = CliRunner().invoke(
        app,
        [
            *offline_arguments(recipe, tmp_path),
            "--image-assets-dir",
            str(roots.preview),
            "--image-cache-dir",
            str(roots.cache),
            "--workers",
            "1",
        ],
    )
    assert result.exit_code == 0, repr(result.exception)
    total = len(assets.images)
    assert image_execution(result.stdout) == {
        "wall_milliseconds": (2 * total + 2) * 1000,
        "cache_hits": total - 1,
        "new_encodings": 1,
        "reuse_milliseconds": (total - 1) * 1000,
        "new_encoding_milliseconds": 1000,
        "workers": 1,
    }
    assert cache.read_bytes() == cached
    assert (tmp_path / "preview/snapshots/preview/current.json").is_file()


def test_offline_cli_builds_every_image_into_empty_roots(
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
    tmp_path: Path,
) -> None:
    recipe, assets, roots = regional_images
    library, cache = tmp_path / "new-library", tmp_path / "new-cache"
    library.mkdir()
    cache.mkdir()
    result = CliRunner().invoke(
        app,
        [
            *offline_arguments(recipe, tmp_path),
            "--image-assets-dir",
            str(library),
            "--image-cache-dir",
            str(cache),
            "--workers",
            "4",
        ],
    )
    assert result.exit_code == 0, repr(result.exception)
    execution = image_execution(result.stdout)
    keys = sorted(p.name for p in cache.rglob("*.json"))
    assert keys == sorted(p.name for p in roots.cache.rglob("*.json"))
    # Byte-identical sources share a cache key; a parallel peer may reuse it.
    encoded = execution["new_encodings"]
    assert isinstance(encoded, int)
    assert len(keys) <= encoded <= len(assets.images)
    assert execution["cache_hits"] == len(assets.images) - encoded
    assert execution["workers"] == 4
    assert {
        p.relative_to(library): p.read_bytes() for p in library.rglob("*.webp")
    } == {
        p.relative_to(roots.preview): p.read_bytes()
        for p in roots.preview.rglob("*.webp")
    }
    assert (tmp_path / "preview/snapshots/preview/current.json").is_file()


@pytest.mark.parametrize("workers", ["0", "5"])
def test_offline_cli_limits_workers_to_four(
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
    tmp_path: Path,
    workers: str,
) -> None:
    recipe, _, roots = regional_images
    library = {p: p.read_bytes() for p in roots.preview.rglob("*") if p.is_file()}
    result = CliRunner().invoke(
        app,
        [
            *offline_arguments(recipe, tmp_path),
            "--image-assets-dir",
            str(roots.preview),
            "--image-cache-dir",
            str(roots.cache),
            "--workers",
            workers,
        ],
    )
    assert result.exit_code == 2
    assert "--workers" in result.output
    assert not (tmp_path / "preview").exists()
    assert not (tmp_path / "bundle").exists()
    assert library == {
        p: p.read_bytes() for p in roots.preview.rglob("*") if p.is_file()
    }


def test_text_only_offline_does_not_depend_on_crop_data(
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
) -> None:
    recipe, _, _ = regional_images
    before = offline.build(recipe)
    (recipe.repo / "authored" / FILE).write_bytes(b"invalid synthetic crop data")
    after = offline.build(recipe)
    assert before.input_content == after.input_content
