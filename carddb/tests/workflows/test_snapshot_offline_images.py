"""Bilingual text, verified image closure, private bundle and preview export."""

import shutil
from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.core.json import digest
from sve_carddb.domains.products import OfficialProducts, ProductIdentities
from sve_carddb.export.image_index import MEDIA_TABLES, ImageIndex, ImagePin
from sve_carddb.export.media import prepare_media
from sve_carddb.images.assets import ImageBuild, PreviewRoots, build_regional_assets
from sve_carddb.images.crops import load_image_crops
from sve_carddb.images.variants import DEFAULT_RECIPE
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.parse.pages import official_en
from sve_carddb.parse.pages.official_jp import image_url
from sve_carddb.workflows import offline
from sve_carddb.workflows.image_index import media_projection

from ..images.test_image_assets_en import FRONT, EnglishImages
from ..images.test_image_assets_en import english_images as english_images  # ruff: ignore[useless-import-alias] -- reuse one sealed EN baseline
from ..support.image_crop_fixtures import install, record
from .test_snapshot_offline import prepared as prepared  # ruff: ignore[useless-import-alias] -- reuse the existing regional text/adoption fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.card_extras import CardPage
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

    for key, card in case.provider.cards.items():
        src = FRONT if key[0] == "en" else "/synthetic/0.png"
        case.provider.cards[key] = card.model_copy(
            update={"image_src": (src,) * len(card.faces)}
        )
    return recipe, assets, roots


def make_index(
    case: Case, recipe: Inputs, assets: ImageBuild, library: Path
) -> ImageIndex:
    projection, bindings = media_projection(case.compose().plan, assets)
    plan = prepare_media(projection, library, revision=1)
    return ImageIndex(
        recipe=DEFAULT_RECIPE.version,
        store_id=recipe.store_id,
        pins=tuple(
            ImagePin(region=pin.region, batch_id=pin.image_batch)
            for pin in recipe.sources
        ),
        evidence=(),
        bindings=bindings,
        tables={key: plan.projection.tables[key] for key in MEDIA_TABLES},
    )


def test_bilingual_text_export_uses_index_without_any_images(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
    tmp_path: Path,
) -> None:
    case, _, _ = prepared
    recipe, assets, roots = regional_images
    index = make_index(case, recipe, assets, roots.preview)
    path = tmp_path / (digest(index.content())[7:] + ".json")
    path.write_bytes(index.content())
    # PNGs, WebPs, cache and producer state are absent when the text command runs.
    shutil.rmtree(roots.preview)
    shutil.rmtree(roots.cache)
    for item in assets.images:
        (recipe.archive / item.source.raw_locator.split(":", 1)[1]).unlink(
            missing_ok=True
        )
    arguments = offline_arguments(recipe, tmp_path)
    result = CliRunner().invoke(app, [*arguments, "--image-index", str(path)])
    assert result.exit_code == 0, repr(result.exception)
    assert not (tmp_path / "preview/images").exists()
    assert not (tmp_path / "private/media-state.json").exists()
    built = offline.build(recipe, image_index=ImageIndex.read(path))
    assert built.projection.tables["printing_image"] == index.tables["printing_image"]
    assert all(row["card_version"] == 1 for row in index.tables["printing_image"])
    assert (tmp_path / "preview/snapshots/preview/current.json").is_file()


@pytest.mark.parametrize("change", ["src", "face", "int_id", "pin"])
def test_index_rejects_wrong_text_binding(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
    change: str,
) -> None:
    case, _, _ = prepared
    recipe, assets, roots = regional_images
    index = make_index(case, recipe, assets, roots.preview)
    if change == "src":
        index.tables["image_asset"][0]["source_src_raw"] = "/wrong.png"
    elif change == "pin":
        index = index.model_copy(update={"store_id": "wrong-store"})
    else:
        first = index.bindings[0]
        first = first.model_copy(
            update={"face_id": "wrong-face"}
            if change == "face"
            else {"int_id": first.int_id + 1}
        )
        index = index.model_copy(update={"bindings": (first, *index.bindings[1:])})
    with pytest.raises(ValueError, match="Image index"):
        offline.build(recipe, image_index=index)


def offline_arguments(recipe: Inputs, root: Path) -> list[str]:
    path = root / "inputs.json"
    path.write_text(recipe.model_dump_json(), encoding="utf-8")
    return [
        "snapshot",
        "export-offline",
        "--inputs",
        str(path),
        "--bundle-dir",
        str(root / "bundle"),
        "--preview-dir",
        str(root / "preview"),
        "--private-dir",
        str(root / "private"),
    ]


def test_independent_image_cli_commits_index_and_reuses_versions(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from sve_carddb.workflows import image_index as producer  # ruff: ignore[import-outside-top-level] -- test the CLI's source adapters with the shared synthetic page provider

    case, _, _ = prepared
    recipe, _, roots = regional_images
    for name in ("FrozenEN", "FrozenJP", "FrozenRegions"):
        monkeypatch.setattr(producer, name, lambda *_args, **_kwargs: None)
    monkeypatch.setattr(
        producer, "plan_preview", lambda *_args, **_kwargs: case.identity
    )
    monkeypatch.setattr(
        producer, "FrozenTexts", lambda *_args, **_kwargs: case.provider
    )
    inputs = tmp_path / "image-inputs.json"
    inputs.write_text(recipe.model_dump_json(), encoding="utf-8")
    output, private = tmp_path / "image-output", tmp_path / "image-private"
    arguments = [
        "images",
        "build-index",
        "--inputs",
        str(inputs),
        "--image-assets-dir",
        str(roots.preview),
        "--image-cache-dir",
        str(roots.cache),
        "--image-output-dir",
        str(output),
        "--private-dir",
        str(private),
    ]
    runner = CliRunner()
    first = runner.invoke(app, arguments)
    assert first.exit_code == 0, repr(first.exception)
    index = next((private / "image-index").glob("*.json"))
    before = index.read_bytes()
    second = runner.invoke(app, arguments)
    assert second.exit_code == 0, repr(second.exception)
    assert '"new_encodings":0' in second.stdout
    assert '"written_images":0' in second.stdout
    assert index.read_bytes() == before
    assert len(list((private / "image-index").glob("*.json"))) == 1
    assert (private / "media-state.json").is_file()
    assert list(output.rglob("*.webp"))
