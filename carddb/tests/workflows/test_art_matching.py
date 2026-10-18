"""The CLI joins a pinned producer index with complete registered identities."""

import io
import json
from typing import TYPE_CHECKING

import pytest
from PIL import Image
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.core.json import digest
from sve_carddb.domains.registry.build import build
from sve_carddb.domains.registry.records import AllocationData, FaceData, PrintingData
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.registry.storage import load, plan_files, write_files
from sve_carddb.export.image_index import ImageBinding, ImageIndex
from sve_carddb.export.media_urls import image_path
from sve_carddb.images.art_matching import ArtCandidates
from sve_carddb.images.variants import DEFAULT_RECIPE, SIZES
from sve_carddb.workflows.art_matching import art_inputs

from ..domains.registry.test_registry import card
from ..domains.registry.test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- dependency of registry fixture
from ..support.registry_snapshot_fixtures import registry_root as registry_root  # ruff: ignore[useless-import-alias] -- shared synthetic identities

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.domains.registry.review import Inputs


def art_bitmap(color: str) -> bytes:
    buffer = io.BytesIO()
    Image.new("RGB", (384, 288), color).save(buffer, "WEBP")
    return buffer.getvalue()


@pytest.fixture
def image_input(
    registry_root: Path, tmp_path: Path, inputs: Inputs
) -> tuple[Path, Path]:
    inputs.jp["DF01-001"] = card("DF01-001", "Synthetic front")
    inputs.jp["DF01-001"].faces.append(card("unused", "Synthetic back").faces[0])
    write_files(plan_files(registry_root, build(inputs, load(registry_root)[1])))
    registry = load_registry(registry_root)
    printings = [
        r.data for r in registry.records.values() if isinstance(r.data, PrintingData)
    ]
    faces = {
        r.data.id: r.data
        for r in registry.records.values()
        if isinstance(r.data, FaceData)
    }
    ids = {
        r.data.printing_id: r.data.int_id
        for r in registry.records.values()
        if isinstance(r.data, AllocationData)
    }
    raw_by_ordinal = (art_bitmap("white"), art_bitmap("black"))
    tables: dict[str, list[dict[str, JsonValue]]] = {
        "image_asset": [],
        "printing_image": [],
        "image_variant": [],
    }
    bindings = []
    root = tmp_path / "images-root"
    for printing in printings:
        for mapping in printing.source_face_map:
            ordinal = faces[mapping.face_id].ordinal
            raw = raw_by_ordinal[ordinal]
            binding = ImageBinding(
                printing_id=printing.id,
                face_id=mapping.face_id,
                int_id=ids[printing.id],
                ordinal=ordinal,
            )
            bindings.append(binding)
            asset = "img:" + printing.id + mapping.face_id
            path = root / image_path(binding.int_id, ordinal, "art_m")
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
            variants: list[JsonValue] = [
                {
                    "size_key": size.key,
                    "width": size.max_width,
                    "height": size.max_height,
                }
                for size in sorted(SIZES, key=lambda s: s.key)
            ]
            tables["image_asset"].append({"id": asset})
            tables["printing_image"].append(
                {
                    "printing_id": printing.id,
                    "face_id": mapping.face_id,
                    "image_id": asset,
                    "variants": variants,
                    "availability": "available",
                    "publication_state": "approved",
                    "card_version": 1,
                    "art_version": 1,
                }
            )
            tables["image_variant"].extend(
                {
                    "image_id": asset,
                    "size_key": size.key,
                    "width": size.max_width,
                    "height": size.max_height,
                    "bytes": len(raw),
                }
                for size in SIZES
            )
    index = ImageIndex(
        recipe=DEFAULT_RECIPE.version,
        store_id="synthetic",
        pins=(),
        evidence=(),
        bindings=tuple(bindings),
        tables=tables,
    )
    path = tmp_path / (digest(index.content())[7:] + ".json")
    path.write_bytes(index.content())
    return path, root


def arguments(
    image_input: tuple[Path, Path], registry_root: Path, output: Path
) -> list[str]:
    index, root = image_input
    return [
        "images",
        "art-candidates",
        "--image-index",
        str(index),
        "--image-root",
        str(root),
        "--authored",
        str(registry_root),
        "--output",
        str(output),
    ]


def test_cli_full_and_incremental_outputs_match(
    image_input: tuple[Path, Path], registry_root: Path, tmp_path: Path
) -> None:
    output = tmp_path.parent / (tmp_path.name + "-output") / "first.json"
    runner = CliRunner()
    first = runner.invoke(app, arguments(image_input, registry_root, output))
    assert first.exit_code == 0, first.output
    assert json.loads(first.stdout)["compared_pairs"] > 0
    second_output = output.with_name("second.json")
    second = runner.invoke(
        app,
        [
            *arguments(image_input, registry_root, second_output),
            "--previous",
            str(output),
        ],
    )
    assert second.exit_code == 0, repr(second.exception)
    assert json.loads(second.stdout)["compared_pairs"] == 0
    assert second_output.read_bytes() == output.read_bytes()
    assert ArtCandidates.read(output).unavailable == ()


def test_back_face_uses_its_own_bitmap(
    image_input: tuple[Path, Path], registry_root: Path
) -> None:
    index_path, root = image_input
    bound, unavailable = art_inputs(ImageIndex.read(index_path), registry_root, root)
    back = next(item for item in bound if item.member.ordinal == 1)
    front = next(
        item
        for item in bound
        if item.member.printing_id == back.member.printing_id
        and item.member.ordinal == 0
    )
    assert back.path.name == f"{back.member.int_id}-f1.webp"
    assert back.member.image_sha256 != front.member.image_sha256
    assert not unavailable


@pytest.mark.parametrize(
    "damage", ["relative", "output_overlap", "missing", "size", "binding", "symlink"]
)
def test_cli_rejects_invalid_inputs_without_writing_candidates(
    image_input: tuple[Path, Path], registry_root: Path, tmp_path: Path, damage: str
) -> None:
    index_path, root = image_input
    output = tmp_path.parent / (tmp_path.name + "-output") / "output.json"
    args = arguments(image_input, registry_root, output)
    bitmap = next(root.rglob("*.webp"))
    if damage == "relative":
        args[3] = "relative-index.json"
    elif damage == "output_overlap":
        output = root / "output.json"
        args[-1] = str(output)
    elif damage == "missing":
        bitmap.unlink()
    elif damage == "size":
        bitmap.write_bytes(b"wrong-size")
    elif damage == "symlink":
        copy = tmp_path / "elsewhere.webp"
        bitmap.replace(copy)
        bitmap.symlink_to(copy)
    else:
        index = ImageIndex.read(index_path)
        first = index.bindings[0]
        changed = first.model_copy(update={"ordinal": first.ordinal + 1})
        index = index.model_copy(update={"bindings": (changed, *index.bindings[1:])})
        index_path = tmp_path / (digest(index.content())[7:] + ".json")
        index_path.write_bytes(index.content())
        args[3] = str(index_path)
    result = CliRunner().invoke(app, args, env={"COLUMNS": "240"})
    assert result.exit_code != 0
    assert not output.exists()
    if damage == "size":
        assert "Art bitmap size differs from pinned image index" in result.output
    elif damage == "binding":
        assert "Art image binding differs from registered face" in result.output


def test_cli_explains_why_previous_cache_is_rejected(
    image_input: tuple[Path, Path], registry_root: Path, tmp_path: Path
) -> None:
    output = tmp_path.parent / (tmp_path.name + "-output") / "first.json"
    runner = CliRunner(env={"COLUMNS": "240"})
    first = runner.invoke(app, arguments(image_input, registry_root, output))
    assert first.exit_code == 0, first.output
    candidate = ArtCandidates.read(output)
    output.write_bytes(
        candidate.model_copy(update={"opencv_version": "obsolete"}).content()
    )
    second = runner.invoke(
        app,
        [
            *arguments(image_input, registry_root, output.with_name("second.json")),
            "--previous",
            str(output),
        ],
    )
    assert second.exit_code != 0
    assert "Previous art inputs or OpenCV changed" in second.output
    assert "run without --previous" in second.output


@pytest.mark.parametrize("field", ["printing_id", "face_id", "image_id"])
def test_cli_identifies_missing_index_references(
    image_input: tuple[Path, Path], registry_root: Path, tmp_path: Path, field: str
) -> None:
    index_path, root = image_input
    index = ImageIndex.read(index_path)
    if field == "image_id":
        del index.tables["printing_image"][0][field]
        expected = "Missing registry or image-index field/reference"
    else:
        index.tables["printing_image"][0][field] = "missing-reference"
        first = index.bindings[0].model_copy(update={field: "missing-reference"})
        index = index.model_copy(update={"bindings": (first, *index.bindings[1:])})
        expected = "Unknown " + field.removesuffix("_id") + " in image index"
    index_path = tmp_path / (digest(index.content())[7:] + ".json")
    index_path.write_bytes(index.content())
    output = tmp_path.parent / (tmp_path.name + "-output") / "output.json"
    result = CliRunner().invoke(
        app,
        arguments((index_path, root), registry_root, output),
        env={"COLUMNS": "240"},
    )
    assert result.exit_code != 0
    assert expected in result.output
    assert (field if field == "image_id" else "missing-reference") in result.output
    assert not output.exists()
