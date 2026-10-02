"""The preview builder binds frozen image provenance into its text build transaction."""

import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

import sve_carddb.snapshot.preview.build as build_module
from sve_carddb.build_inputs import InputRecord
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.image_assets import (
    PARSER,
    ImageReference,
    PreviewRoots,
    build_jp_assets,
    populate_jp_assets,
)
from sve_carddb.registry.records import PrintingData
from sve_carddb.snapshot.export import export_snapshot
from sve_carddb.snapshot.preview import Roots, write_preview
from sve_carddb.snapshot.preview.build import build
from sve_carddb.snapshot.values import array, digest, integer, object_value, string

from .shared_case_fixtures import TextCaseTemplate
from .test_registry import make_inputs
from .test_snapshot_preview import prepare_build
from .text_observation_fixtures import make_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import SourceUse
    from sve_carddb.image_assets import ImageBuild
    from sve_carddb.snapshot.preview.build import Inputs

    from .text_observation_fixtures import Case


@pytest.fixture(scope="module")
def double_text_case(tmp_path_factory: pytest.TempPathFactory) -> TextCaseTemplate:
    base = tmp_path_factory.mktemp("double-image-text-case")
    inputs = make_inputs()
    for card in (*inputs.jp.values(), *inputs.en.values()):
        card.faces.append(card.faces[0].model_copy(deep=True))
    return TextCaseTemplate.capture(make_case(base / "authored", inputs), base)


@pytest.fixture(scope="module")
def encoded_images(
    image_archive_template: tuple[Path, str, str],
    tmp_path_factory: pytest.TempPathFactory,
) -> tuple[ImageBuild, PreviewRoots]:
    root = tmp_path_factory.mktemp("encoded-preview-images")
    roots = PreviewRoots(root / "library", root / "formal", root / "cache")
    return build_jp_assets(FrozenSources(*image_archive_template), roots), roots


@pytest.fixture
def prepared_image_build(
    double_text_case: TextCaseTemplate,
    image_archive_template: tuple[Path, str, str],
    encoded_images: tuple[ImageBuild, PreviewRoots],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> tuple[Case, Inputs, ImageBuild, PreviewRoots, tuple[ImageReference, ...]]:
    case = double_text_case.copy(tmp_path / "synthetic")
    shutil.copytree(image_archive_template[0], case.store, dirs_exist_ok=True)
    images, image_roots = encoded_images
    recipe = prepare_build(case, tmp_path, monkeypatch)
    recipe = recipe.model_copy(
        update={
            "image_batch": image_archive_template[2],
            "card_batch": next(
                iter(case.identity.evidence.values())
            ).source.archive.batch_id,
        }
    )
    references = tuple(
        ImageReference(
            record.data.id,
            mapping.face_id,
            record.data.card_no,
            case.identity.evidence["jp", record.data.card_no].source.model_copy(
                update={"parser_version": PARSER}
            ),
            "/synthetic/1.png" if mapping.source_index else "/synthetic/0.png",
            "https://shadowverse-evolve.com/synthetic/1.png"
            if mapping.source_index
            else "https://shadowverse-evolve.com/synthetic/0.png",
        )
        for record in case.identity.included("printing")
        if isinstance(record.data, PrintingData) and record.data.region == "jp"
        for mapping in record.data.source_face_map
    )
    assert any(ref.source_src_raw == "/synthetic/1.png" for ref in references)
    # TextCase has synthetic JSON pages; exercise the real importer after its HTML boundary.
    monkeypatch.setattr(build_module, "plan_jp_images", lambda *_args: references)
    return case, recipe, images, image_roots, references


def test_build_mounts_images_with_exact_source_closure_and_two_faces(
    prepared_image_build: tuple[
        Case, Inputs, ImageBuild, PreviewRoots, tuple[ImageReference, ...]
    ],
    tmp_path: Path,
) -> None:
    case, recipe, images, image_roots, references = prepared_image_build
    built = build(recipe, images=images, image_root=image_roots.preview)
    tables = built.projection.tables
    assert len(tables["printing_image"]) == len(references)
    assert len(tables["image_asset"]) == 2
    assert len(tables["image_variant"]) == 10
    assert {row["size_key"] for row in tables["image_variant"]} == {
        "card_s",
        "card_m",
        "card_l",
        "art_s",
        "art_m",
    }
    two_faces = next(row for row in tables["printing"] if len(array(row["faces"])) == 2)
    assert {
        row["face_id"]
        for row in tables["printing_image"]
        if row["printing_id"] == two_faces["id"]
    } == {object_value(row)["face_id"] for row in array(two_faces["faces"])}
    record = InputRecord.model_validate_json(built.input_content)
    expected_sources = {
        use.source.id for use in (*case.plan.source_uses(), *images.source_uses())
    }
    assert expected_sources <= {use.source.id for use in record.uses}
    assert {use.source.id for use in record.uses if use.usage == "jp_image_link"} == {
        ref.page.id for ref in references
    }
    assert built.report["input_sha256"] == digest(built.input_content)
    assert "image manifest integration (#35)" not in array(
        built.report["incomplete_formal_gates"]
    )
    assert built.projection.metadata["source_windows"] == []
    snapshot = export_snapshot(built.projection, built.ownership, recipe.batch())
    report = write_preview(
        snapshot,
        Roots(tmp_path / "output", image_roots.cdn),
        built.report,
        image_source=image_roots.preview,
        confirmed_images=built.confirmed_images,
    )
    assert integer(object_value(report["images"])["unique_files"]) > 0
    assert all(
        (tmp_path / "output" / string(row["path"])).is_file()
        for row in tables["image_variant"]
    )
    with pytest.raises(ValueError, match="pinned image batch"):
        build(
            recipe.model_copy(update={"image_batch": "sha256:" + "c" * 64}),
            images=images,
            image_root=image_roots.preview,
        )
    with pytest.raises(ValueError, match="provided together"):
        build(recipe, images=images)
    forged = replace(
        images, images=(replace(images.images[0], raw_bytes=0), *images.images[1:])
    )
    with pytest.raises(ValueError, match="source bytes"):
        build(recipe, images=forged, image_root=image_roots.preview)


@pytest.mark.parametrize("usage", ["jp_image_link", "jp_image_variant"])
def test_build_rejects_missing_image_source_use_after_real_population(
    prepared_image_build: tuple[
        Case, Inputs, ImageBuild, PreviewRoots, tuple[ImageReference, ...]
    ],
    monkeypatch: pytest.MonkeyPatch,
    usage: str,
) -> None:
    _, recipe, images, image_roots, _ = prepared_image_build

    def omit_use(
        db: Database,
        images: ImageBuild,
        references: tuple[ImageReference, ...],
        root: Path,
    ) -> tuple[SourceUse, ...]:
        uses = populate_jp_assets(db, images, references, root)
        omitted = next(use for use in uses if use.usage == usage)
        # Keep the real DB graph intact while corrupting only the returned input record.
        return tuple(use for use in uses if use != omitted)

    monkeypatch.setattr(build_module, "populate_jp_assets", omit_use)
    with pytest.raises(
        ValueError, match=r"^Build input use closure or context mismatch$"
    ):
        build(recipe, images=images, image_root=image_roots.preview)
