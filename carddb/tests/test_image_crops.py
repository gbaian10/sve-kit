"""Every crop rejection uses a small counterexample and an anchored error match."""

import json
import re
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.image_crops import load_image_crops, parse_crops
from sve_carddb.image_variants import CropBox
from sve_carddb.registry.storage import MAX_BYTES

from .image_crop_fixtures import install, record

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.source_archive import Descriptor

AUTHORED = Path(__file__).resolve().parents[2] / "authored"
# Adopted tops from the former sharded format, where every row spelled out 36/384/288.
GOLDEN_TOPS = {
    "BP10-001": 184,
    "BP10-002": 184,
    "BP10-SL01": 184,
    "BP10-SL02": 184,
    "BP10-U01": 232,
    "BP15-PR01": 256,
    "BP10-001EN": 184,
    "BP10-002EN": 184,
    "BP10-SL01EN": 184,
    "BP10-SL02EN": 184,
    "BP10-U01EN": 232,
    "BP15-PR01EN": 256,
}


@pytest.fixture(scope="module")
def descriptor(image_archive_template: tuple[Path, str, str]) -> Descriptor:
    frozen = FrozenSources(*image_archive_template)
    return frozen.descriptor(frozen.inventory.current[0].source_version_id)


def test_authored_boxes_match_adopted_golden() -> None:
    records = load_image_crops(AUTHORED).records.values()
    assert {r.card_no: r.box() for r in records if r.card_no in GOLDEN_TOPS} == {
        card_no: CropBox(36, top, 384, 288) for card_no, top in GOLDEN_TOPS.items()
    }


def test_working_tree_edit_applies_without_git(
    tmp_path: Path, descriptor: Descriptor
) -> None:
    install(tmp_path, [record(descriptor)])
    assert load_image_crops(tmp_path).box(descriptor) == CropBox(4, 24, 64, 48)
    install(tmp_path, [record(descriptor) | {"top": 30}])
    assert load_image_crops(tmp_path).box(descriptor) == CropBox(4, 30, 64, 48)


def test_omitted_columns_use_the_shared_box(
    tmp_path: Path, descriptor: Descriptor
) -> None:
    row = {
        key: value
        for key, value in record(descriptor).items()
        if key not in {"left", "width", "height"}
    }
    install(tmp_path, [row])
    assert load_image_crops(tmp_path).box(descriptor) == CropBox(36, 24, 384, 288)


def test_missing_file_is_not_an_empty_set(tmp_path: Path) -> None:
    with pytest.raises(FileNotFoundError):
        load_image_crops(tmp_path)
    install(tmp_path, [])
    assert load_image_crops(tmp_path).records == {}


@pytest.mark.parametrize("symlink_root", [False, True], ids=["file", "authored-root"])
def test_symlinked_crop_input_is_rejected(tmp_path: Path, symlink_root: bool) -> None:
    external = tmp_path / "external"
    external.mkdir()
    (external / "image-crops.yaml").write_text("[]\n")
    authored = tmp_path / "authored"
    authored.mkdir()
    if symlink_root:
        authored.rmdir()
        authored.symlink_to(external, target_is_directory=True)
    else:
        (authored / "image-crops.yaml").symlink_to(external / "image-crops.yaml")

    with pytest.raises(
        ValueError, match=r"^Symlinks are forbidden in image crop inputs$"
    ):
        load_image_crops(authored)


@pytest.mark.parametrize(
    ("field", "value", "location"),
    [
        ("left", True, "0.left:int_type"),
        ("left", -1, "0.left:greater_than_equal"),
        ("top", "24", "0.top:int_type"),
        ("width", 0, "0.width:greater_than"),
        ("height", -1, "0.height:greater_than"),
        ("width", 65, "0:value_error"),
        ("reason", "  ", "0.reason:value_error"),
        ("card_no", "  ", "0.card_no:value_error"),
        ("region", "tw", "0.region:literal_error"),
        ("source_key", "img:v1:" + "a" * 64, "0.source_key:string_pattern_mismatch"),
        (
            "source_sha256",
            "sha256:" + "a" * 64,
            "0.source_sha256:string_pattern_mismatch",
        ),
        ("image_id", "img:binding:" + "a" * 64, "0.image_id:extra_forbidden"),
    ],
)
def test_record_strict_fields(
    descriptor: Descriptor, field: str, value: JsonValue, location: str
) -> None:
    row = record(descriptor) | {field: value}
    with pytest.raises(
        ValueError,
        match="^" + re.escape("Invalid image crop fields: " + location) + "$",
    ):
        parse_crops(json.dumps([row]).encode())


@pytest.mark.parametrize(
    ("raw", "message"),
    [
        (b"- top: 1\n  top: 2\n", "Invalid authored YAML syntax"),
        (b"- left: 4.0\n", "Floating point JSON is forbidden"),
        (b" " * MAX_BYTES, "Oversized image crop YAML"),
    ],
    ids=["duplicate-key", "float", "oversized"],
)
def test_yaml_boundary_is_not_bypassed(raw: bytes, message: str) -> None:
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        parse_crops(raw)


def test_duplicate_key_is_rejected(tmp_path: Path, descriptor: Descriptor) -> None:
    row = record(descriptor)
    install(tmp_path, [row, row | {"card_no": "TEST-002"}])
    with pytest.raises(ValueError, match=r"^Duplicate image crop key$"):
        load_image_crops(tmp_path)


def test_new_bytes_fail_closed_and_new_resources_default(
    tmp_path: Path, descriptor: Descriptor
) -> None:
    install(tmp_path, [record(descriptor)])
    crops = load_image_crops(tmp_path)
    changed = descriptor.model_copy(update={"raw_sha256": "sha256:" + "b" * 64})
    with pytest.raises(
        ValueError,
        match=r"^Unadopted crop source version: source_key=sha256:[a-f0-9]{64} new_hash=b{64} adopted_hashes=[a-f0-9]{64}$",
    ):
        crops.box(changed)
    unrelated = changed.model_copy(update={"source_key": "sha256:" + "c" * 64})
    assert crops.box(unrelated) is None


def test_new_version_can_adopt_a_restored_default_box(
    tmp_path: Path, descriptor: Descriptor
) -> None:
    old = record(descriptor)
    install(tmp_path, [old, old | {"source_sha256": "b" * 64, "left": 6, "top": 15}])
    crops = load_image_crops(tmp_path)
    restored = descriptor.model_copy(update={"raw_sha256": "sha256:" + "b" * 64})
    assert crops.box(restored) == CropBox(6, 15, 64, 48)
    assert crops.box(descriptor) == CropBox(4, 24, 64, 48)


def test_unused_en_record_is_valid(tmp_path: Path, descriptor: Descriptor) -> None:
    row = record(descriptor)
    en = row | {
        "region": "en",
        "card_no": "TEST-001EN",
        "source_key": "sha256:" + "e" * 64,
    }
    install(tmp_path, [row, en])
    crops = load_image_crops(tmp_path)
    assert {r.region for r in crops.records.values()} == {"jp", "en"}
