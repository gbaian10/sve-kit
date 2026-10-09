"""Sealed invented EN pages prove independent URLs, faces, crops and region gates."""

import shutil
from dataclasses import dataclass, replace
from datetime import date
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.registry.build import build
from sve_carddb.domains.registry.inputs import Mapping
from sve_carddb.domains.registry.preview import (
    FrozenEN,
    FrozenJP,
    FrozenRegions,
    plan_preview,
)
from sve_carddb.domains.registry.review import InitDecisions, Inputs
from sve_carddb.domains.registry.storage import plan_files, write_files
from sve_carddb.images.assets import (
    ImageBuild,
    PreviewRoots,
    build_regional_assets,
    plan_regional_images,
    populate_assets,
    verify_asset_sources,
)
from sve_carddb.images.crop_report import crop_report
from sve_carddb.images.crops import load_image_crops
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import ArchiveStore, Scope, seal_batch
from sve_carddb.parse.pages import official_en
from sve_carddb.parse.pages.extract_en import extract_card as extract_en
from sve_carddb.parse.pages.extract_jp import extract_card
from sve_carddb.parse.pages.official_jp import image_url

from .en_extract_fixtures import page
from .image_crop_fixtures import install, record
from .registry_observation_fixtures import parsed_card
from .test_image_assets_db import Staged, make_staged
from .test_image_variants import png
from .test_source_archive import _put, _resource

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.images.crops import ImageCrops

NUMBER = "TEST-01ⓈaEN"
FRONT = "/synthetic/front .png"
BACK = "/synthetic/back.png"


@dataclass(frozen=True)
class EnglishImages:
    parents: Staged
    cards: FrozenSources
    images: FrozenSources
    encoded: ImageBuild
    roots: PreviewRoots
    crops: ImageCrops


@pytest.fixture(scope="module")
def english_images(  # ruff: ignore[too-many-locals] -- seal the shared minimal bilingual baseline once per module
    tmp_path_factory: pytest.TempPathFactory,
    image_archive_template: tuple[Path, str, str],
) -> EnglishImages:
    base = tmp_path_factory.mktemp("english-images")
    original, store_id, batch = image_archive_template
    shutil.copytree(original.parent, base / "input")
    jp_images = FrozenSources(base / "input/archive", store_id, batch)
    jp = make_staged(base, jp_images)
    data_root = jp_images.root.parent / "data"
    store = ArchiveStore(
        data_root,
        data_root / "manifest/manifest.sqlite",
        data_root / "manifest/.lock",
        jp_images.root,
        store_id,
    )
    original_src = ("../exact/" + NUMBER + ".png?v=7").encode()
    raw = (
        page(NUMBER, double=True)
        .replace(b"Synthetic class", b"Forestcraft")
        .replace(b"Synthetic type", b"Follower")
        .replace(b"Cost</span>-", b"Cost</span>1")
        .replace(b"Power</span>02", b"Power</span>1")
        .replace(b"HP</span>X", b"HP</span>1")
        .replace(original_src, FRONT.encode(), 1)
        .replace(original_src, BACK.encode(), 1)
    )
    _put(
        store,
        replace(
            _resource(official_en.card_url(NUMBER), "raw/en.html", raw, Kind.CARD),
            region=SourceRegion.EN,
        ),
        raw,
    )
    cards_pin = seal_batch(store, scope=(Scope(provider="en", kind="card"),))
    cards = FrozenSources(store.root, store_id, cards_pin.batch_id)
    for index, src in enumerate((FRONT, BACK)):
        data = png(80, 112)
        _put(
            store,
            replace(
                _resource(
                    image_url(src, official_en.card_url(NUMBER)),
                    f"raw/en-{index}.png",
                    data,
                ),
                region=SourceRegion.EN,
            ),
            data,
        )
    images_pin = seal_batch(store, scope=(Scope(provider="en", kind="image"),))
    images = FrozenSources(store.root, store_id, images_pin.batch_id)
    authored = base / "regional/authored"
    jp_cards = {}
    for number in ("TEST-001", "TEST-002"):
        evidence = jp.plan.evidence["jp", number]
        _, content, _ = jp.cards.read(evidence.source.id, parser_version="synthetic-v1")
        jp_cards[number] = parsed_card(extract_card(content, number=number))
    registry = Inputs(
        jp=jp_cards,
        en={NUMBER: parsed_card(extract_en(raw, number=NUMBER))},
        mapping=Mapping(targets={NUMBER: "TEST-002"}, original_art=set(), reskins={}),
        decisions=InitDecisions(),
        as_of=date(2026, 9, 29),
        jp_hash="sha256:" + "0" * 64,
    )
    write_files(plan_files(authored, build(registry, {})))
    plan = plan_preview(
        authored,
        FrozenRegions(
            jp=FrozenJP(
                store.root, store_id, jp.cards.batch_id, parser_version="synthetic-v1"
            ),
            en=FrozenEN(
                store.root, store_id, cards.batch_id, parser_version="synthetic-v1"
            ),
        ),
        regions=("en", "jp"),
    )
    crop_repo = base / "crop-repo"
    install(
        crop_repo / "authored",
        [
            record(images.descriptor(images.inventory.current[0].source_version_id))
            | {"region": "en", "card_no": NUMBER}
        ],
    )
    crops = load_image_crops(crop_repo / "authored")
    roots = PreviewRoots(base / "library", base / "cache")
    encoded = build_regional_assets(images, roots, region="en", crops=crops, workers=2)
    return EnglishImages(
        Staged(jp.cards, plan, jp.context), cards, images, encoded, roots, crops
    )


def test_en_original_src_double_faces_and_crop_report(
    english_images: EnglishImages,
) -> None:
    case = english_images
    with create_database(compile_build(("images", "en"))) as db:
        with db.transaction():
            case.parents.parents(db)
        references = plan_regional_images(
            db, case.parents.plan, case.cards, region="en"
        )
        assert len(references) == 2
        assert {ref.region for ref in references} == {"en"}
        assert {ref.source_src_raw for ref in references} == {FRONT, BACK}
        assert {ref.source_url for ref in references} == {
            image_url(src, official_en.card_url(NUMBER)) for src in (FRONT, BACK)
        }
        with db.transaction():
            uses = populate_assets(db, case.encoded, references, case.roots.preview)
        assert {use.usage for use in uses} == {"en_image_link", "en_image_variant"}
        assert len(db.rows("printing_image")) == 2
        assert len(db.rows("image_variant")) == 10
        report = crop_report(case.crops, case.encoded, references, db)
        assert report["applied_source_images"] == 1
        assert report["annotation_mismatches"] == []
        assert report["unused"] == []
    verify_asset_sources(
        case.encoded, {case.images.store_id: case.images.root}, crops=case.crops
    )
    reused = build_regional_assets(
        case.images,
        case.roots,
        region="en",
        crops=case.crops,
        workers=2,
    )
    assert all(item.result.cache_hit for item in reused.images)


def test_en_declared_region_and_binding_cannot_cross_jp(
    english_images: EnglishImages,
) -> None:
    case = english_images
    with pytest.raises(ValueError, match="exclusively JP"):
        build_regional_assets(case.images, case.roots, region="jp", crops=case.crops)
    forged = replace(
        case.encoded,
        images=tuple(replace(item, region="jp") for item in case.encoded.images),
    )
    with pytest.raises(ValueError, match="regional batch"):
        verify_asset_sources(
            forged, {case.images.store_id: case.images.root}, crops=case.crops
        )
    with create_database(compile_build(("images", "en"))) as db:
        with db.transaction():
            case.parents.parents(db)
        refs = plan_regional_images(db, case.parents.plan, case.cards, region="en")
        with pytest.raises(ValueError, match="binding"), db.transaction():
            populate_assets(
                db, case.encoded, (replace(refs[0], region="jp"),), case.roots.preview
            )
        assert not db.rows("image_asset")
