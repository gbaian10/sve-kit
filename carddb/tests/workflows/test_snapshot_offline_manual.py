"""Unlisted manual printings flow through the offline build, export and image index."""

import hashlib
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import array, canonical, object_value, parse, string
from sve_carddb.core.provenance import InputRecord
from sve_carddb.domains.manual_printings import (
    ManualSpec,
    load_manual_printings,
    plan_manual_files,
)
from sve_carddb.domains.products import load_products
from sve_carddb.domains.registry.storage import write_files
from sve_carddb.export.image_index import MEDIA_TABLES, ImageIndex, ImagePin
from sve_carddb.export.media import prepare_media
from sve_carddb.export.reader import read_snapshot
from sve_carddb.export.transport import export_snapshot
from sve_carddb.images.manual import build_manual_assets
from sve_carddb.images.variants import DEFAULT_RECIPE
from sve_carddb.workflows import offline
from sve_carddb.workflows.image_index import media_projection

from ..images.test_image_variants import png
from ..support.english_image_fixtures import english_images as english_images  # ruff: ignore[useless-import-alias] -- dependency of the sealed image baseline
from ..support.product_fixtures import envelope, family, install
from .test_snapshot_offline import prepared as prepared  # ruff: ignore[useless-import-alias] -- reuse the regional text/adoption fixture
from .test_snapshot_offline_images import regional_images as regional_images  # ruff: ignore[useless-import-alias] -- reuse the sealed image baseline

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.domains.card_extras import CardPage
    from sve_carddb.domains.manual_printings import ManualSnapshot
    from sve_carddb.images.assets import ImageBuild, PreviewRoots
    from sve_carddb.workflows.offline import Inputs

    from ..support.text_observation_fixtures import Case

MASTER = png(80, 112)
REFERENCES = (
    "https://shadowverse-evolve.com/news/post-synthetic/",
    "https://x.com/shadowverse_ev/status/1715598409107939492",
)


def spec(*, with_image: bool = True, **changes: JsonValue) -> ManualSpec:
    base: dict[str, JsonValue] = {
        "slug": "synthetic-event-ep",
        "int_id": 55000,
        "region": "jp",
        "home_set_id": "EP",
        "card_type": "ep",
        "card_no": None,
        "card_no_state": "none",
        "printed_code": "2024合成-A",
        "name": {"lang": "ja", "text": "合成EP"},
        "release_kind": "event",
        "references": list(REFERENCES),
        "image": {
            "sha256": "sha256:" + hashlib.sha256(MASTER).hexdigest(),
            "width": 80,
            "height": 112,
            "origin": "maintainer_photo",
            "source_url": None,
            "approved": True,
        }
        if with_image
        else None,
    }
    return ManualSpec.model_validate_json(canonical(base | changes))


def register(case: Case, recipe: Inputs, *specs: ManualSpec) -> ManualSnapshot:
    """Append manual shards without changing the official registry."""
    root = recipe.repo / "authored"
    existing = load_manual_printings(root)
    write_files(plan_manual_files(root, existing, specs).files)
    install(case.root, "products/family/EP/001.yaml", envelope([family("EP")]))
    case.catalog = load_products(case.root, registry=case.identity.snapshot)
    return load_manual_printings(
        root,
        registry=case.identity.snapshot,
    )


def test_offline_build_exports_textless_unlisted_printing(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]], tmp_path: Path
) -> None:
    case, recipe, _ = prepared
    manual = register(case, recipe, spec())
    data = manual.printings()[0]
    built = offline.build(recipe, bundle_dir=tmp_path / "bundle")
    tables = built.projection.tables
    assert_manual_printing(tables, data.id, data.face_id)
    assert_provisional_card(tables, data.id, data.card_id, data.face_id)
    assert built.report["manual_printings"] == {"printings": 1, "with_image": 1}
    record = InputRecord.model_validate_json(built.input_content)
    pinned = object_value(parse(record.context.configuration.encode()))
    assert [
        string(object_value(item)["path"]) for item in array(pinned["manual_printings"])
    ] == ["manual-printings/printings/EP/001.yaml"]
    snapshot = export_snapshot(
        prepare_media(built.projection, None, revision=1).projection,
        built.ownership,
        recipe.batch(),
    )
    joined = read_snapshot(
        snapshot.manifest, {key: blob.raw for key, blob in snapshot.payloads.items()}
    )
    assert joined == tables


def assert_manual_printing(
    tables: dict[str, list[dict[str, JsonValue]]], printing_id: str, face_id: str
) -> None:
    printing = next(row for row in tables["printing"] if row["id"] == printing_id)
    assert not printing["card_no"]
    assert printing["card_no_state"] == "none"
    assert printing["catalog_state"] == "unlisted"
    assert printing["manual_type_code"] == "ep"
    assert printing["printed_code"] == "2024合成-A"
    assert printing["reference_urls"] == sorted(REFERENCES)
    assert printing["int_id"] == 55000
    assert printing["decklog_available"] is False
    assert printing["listing_confidence"] == "high"
    name = next(
        row["text"]
        for row in tables["text_unit"]
        if row["id"] == printing["manual_name_unit_id"]
    )
    assert name == "合成EP"
    [face] = [object_value(item) for item in array(printing["faces"])]
    assert face["printed_text_state"] == "unknown"
    assert face["printed_name_unit_id"] is None
    assert face["observations"] == []
    assert face["face_id"] == face_id
    # Every official printing still carries its official number and no manual fields.
    for row in tables["printing"]:
        if row["id"] != printing_id:
            assert row["card_no_state"] == "official"
            assert row["printed_code"] is None
            assert row["manual_name_unit_id"] is None
            assert row["manual_type_code"] is None


def assert_provisional_card(
    tables: dict[str, list[dict[str, JsonValue]]],
    printing_id: str,
    card_id: str,
    face_id: str,
) -> None:
    public_face = next(row for row in tables["face"] if row["id"] == face_id)
    assert public_face["current"] == []
    assert public_face["wording"] == []
    assert not any(row["face_id"] == face_id for row in tables["face_revision"])
    assert not any(row["face_id"] == face_id for row in tables["face_rules_name"])
    card = next(row for row in tables["card"] if row["id"] == card_id)
    assert card["identity_state"] == "provisional"
    regions = {
        object_value(item)["region"]: object_value(item)
        for item in array(card["regions"])
    }
    assert regions["jp"]["default_printing_id"] == printing_id
    assert regions["jp"]["default_method"] == "fallback"
    assert regions["jp"]["deck_role"] is None
    support = next(
        row for row in tables["card_engine_support"] if row["card_id"] == card_id
    )
    blocks = {
        object_value(raw)["region"]: array(object_value(raw)["reasons"])
        for raw in array(support["region_blocks"])
    }
    assert {"identity_unconfirmed", "missing_region_source"} <= set(blocks["jp"])
    assert "wording_pending" not in blocks["jp"]


def make_index(
    case: Case,
    recipe: Inputs,
    assets: ImageBuild,
    roots: PreviewRoots,
    manual: ManualSnapshot,
    masters: Path | None,
) -> ImageIndex:
    manual_images = build_manual_assets(manual, masters, roots)
    projection, bindings, _ = media_projection(
        case.compose().plan, assets, manual_images
    )
    plan = prepare_media(projection, roots.preview, revision=1)
    index = ImageIndex(
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
    index.validate_tables()
    return index


def test_manual_masters_join_the_image_index(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
    tmp_path: Path,
) -> None:
    case, _, _ = prepared
    recipe, assets, roots = regional_images
    masters = tmp_path / "masters"
    (masters / "nested").mkdir(parents=True)
    # File names carry no meaning; the master is found by its SHA-256 alone.
    (masters / "nested" / "whatever.png").write_bytes(MASTER)
    manual = register(
        case,
        recipe,
        spec(),
        spec(
            slug="synthetic-missing-master",
            int_id=55001,
            printed_code=None,
            with_image=False,
        ),
    )
    with pytest.raises(ValueError, match="require --manual-image-dir"):
        build_manual_assets(manual, None, roots)
    index = make_index(case, recipe, assets, roots, manual, masters)
    built = offline.build(recipe, image_index=index)
    assert_manual_media(built.projection.tables, manual)
    assert built.report["manual_printings"] == {"printings": 2, "with_image": 1}
    texts = case.compose().plan
    with_image = next(item for item in manual.printings() if item.image is not None)
    for field, value in (
        ("source_src_raw", "sha256:" + "0" * 64),
        ("width", 81),
        ("height", 113),
        ("format", "webp"),
        ("id", "img:manual:" + "0" * 64),
    ):
        tampered = index.model_copy(deep=True)
        row = next(
            item
            for item in tampered.tables["printing_image"]
            if item["printing_id"] == with_image.id
        )
        asset = next(
            item
            for item in tampered.tables["image_asset"]
            if item["id"] == row["image_id"]
        )
        if field == "id":
            for variant in tampered.tables["image_variant"]:
                if variant["image_id"] == asset["id"]:
                    variant["image_id"] = value
            row["image_id"] = value
        asset[field] = value
        with pytest.raises(ValueError, match="manual asset"):
            tampered.attach(built.projection, texts, manual)
    tampered = index.model_copy(deep=True)
    row = next(
        item
        for item in tampered.tables["printing_image"]
        if item["printing_id"] == with_image.id
    )
    row["publication_state"] = "pending"
    with pytest.raises(ValueError, match="manual asset"):
        tampered.attach(built.projection, texts, manual)


def assert_manual_media(
    tables: dict[str, list[dict[str, JsonValue]]], manual: ManualSnapshot
) -> None:
    rows = {string(row["printing_id"]): row for row in tables["printing_image"]}
    assets_by_id = {string(row["id"]): row for row in tables["image_asset"]}
    with_image = next(item for item in manual.printings() if item.image is not None)
    without = next(item for item in manual.printings() if item.image is None)
    available = rows[with_image.id]
    assert (available["publication_state"], available["availability"]) == (
        "approved",
        "available",
    )
    assert sorted(
        string(object_value(v)["size_key"]) for v in array(available["variants"])
    ) == ["art_s", "card_l", "card_s"]
    asset = assets_by_id[string(available["image_id"])]
    assert asset["source_src_raw"] == "sha256:" + hashlib.sha256(MASTER).hexdigest()
    assert asset["source_url"] is None
    assert (asset["width"], asset["height"], asset["format"]) == (80, 112, "png")
    pending = rows[without.id]
    assert (pending["publication_state"], pending["availability"]) == (
        "pending",
        "missing",
    )
    assert pending["variants"] == []
    assert assets_by_id[string(pending["image_id"])]["source_src_raw"] == (
        "manual:" + without.slug
    )


def test_manual_master_dimensions_must_match_the_record(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
    tmp_path: Path,
) -> None:
    case, _, _ = prepared
    recipe, _, roots = regional_images
    masters = tmp_path / "masters"
    masters.mkdir()
    (masters / "master.png").write_bytes(MASTER)
    manual = register(
        case,
        recipe,
        spec(
            image=dict(
                object_value(spec().model_dump(mode="json")["image"]) | {"height": 113}
            )
        ),
    )
    with pytest.raises(ValueError, match="dimensions differ"):
        build_manual_assets(manual, masters, roots)
    (masters / "master.png").unlink()
    with pytest.raises(ValueError, match="Missing master PNG"):
        build_manual_assets(manual, masters, roots)
