"""Every crop rejection uses a small counterexample and an anchored error match."""

import json
import re
import shutil
from dataclasses import dataclass
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_inputs import BuildContext
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.image_crops import (
    CropApproval,
    CropShard,
    ImageCrops,
    _model,
    load_image_crops,
)
from sve_carddb.image_variants import CropBox
from sve_carddb.registry.storage import MAX_BYTES

from .adoption_fixtures import commit
from .image_crop_fixtures import (
    RECEIPT,
    SHARD,
    initialize,
    install,
    record,
    version,
    write,
)

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.source_archive import Descriptor


@dataclass(frozen=True)
class CropCase:
    repo: Path
    revision: str
    descriptor: Descriptor

    @property
    def root(self) -> Path:
        return self.repo / "authored"

    def load(self) -> ImageCrops:
        return load_image_crops(self.root, authored_revision=self.revision)


@pytest.fixture(scope="module")
def crop_template(
    tmp_path_factory: pytest.TempPathFactory,
    image_archive_template: tuple[Path, str, str],
) -> CropCase:
    repo = tmp_path_factory.mktemp("crop-adoption-template")
    frozen = FrozenSources(*image_archive_template)
    descriptor = frozen.descriptor(frozen.inventory.current[0].source_version_id)
    install(repo / "authored", [record(descriptor)])
    revision = initialize(repo)
    return CropCase(repo, revision, descriptor)


@pytest.fixture
def crop_case(crop_template: CropCase, tmp_path: Path) -> CropCase:
    repo = tmp_path / "repo"
    shutil.copytree(crop_template.repo, repo)
    return CropCase(repo, crop_template.revision, crop_template.descriptor)


def test_exact_pins_and_source_derived_id(crop_template: CropCase) -> None:
    crops = crop_template.load()
    assert list(crops.files) == sorted(crops.files)
    assert set(crops.dependencies()) == {"authored/" + SHARD, "authored/" + RECEIPT}
    override = crops.override(crop_template.descriptor)
    assert override is not None
    assert override.image_id.startswith("img:v1:")
    assert override.image_id != "img:binding:" + override.image_id[7:]
    assert CropBox(
        override.left, override.top, override.width, override.height
    ).bounds == (4, 24, 68, 72)
    context = BuildContext.from_inputs(
        crop_template.revision,
        crops.dependencies(),
        {"image_crop_overrides": crops.configuration()},
    )
    crops.verify_context(context)


def test_empty_directory_is_pinned_not_a_missing_input_shortcut(tmp_path: Path) -> None:
    (tmp_path / "keep").write_text("synthetic")
    revision = initialize(tmp_path)
    crops = load_image_crops(tmp_path / "authored", authored_revision=revision)
    assert crops.records == {}
    assert crops.configuration()["files"] == []
    write(tmp_path / "authored" / SHARD, {})
    with pytest.raises(
        ValueError, match=r"^Image crop file closure differs from pinned revision$"
    ):
        load_image_crops(tmp_path / "authored", authored_revision=revision)


@pytest.mark.parametrize(
    ("case", "message"),
    [
        ("missing", "Image crop file closure differs from pinned revision"),
        ("extra", "Image crop file closure differs from pinned revision"),
        ("dirty", "Image crop bytes differ from pinned authored revision"),
        ("symlink-file", "Symlinks are forbidden in image crop inputs"),
        ("symlink-dir", "Symlinks are forbidden in image crop inputs"),
        ("git-symlink", "Image crop Git inputs must be regular files"),
        ("yml", "Unsafe image crop input path"),
        ("index", "Unsafe image crop input path"),
        ("receipt-shard", "Unsafe image crop input path"),
        ("kind", "Invalid image crop fields: kind:literal_error"),
        ("duplicate", "Duplicate global image crop key"),
        ("receipt-name", "Crop receipt ID differs from filename"),
        ("receipt-missing", "Image crop approval receipt is missing"),
        ("member-hash", "Image crop approval source or box mismatch"),
        ("member-box", "Image crop approval source or box mismatch"),
        ("oversized", "Oversized image crop YAML"),
        ("members-duplicate", "Invalid image crop fields: :value_error"),
        ("sorted", "Image crop shard records must be sorted"),
    ],
)
def test_complete_inventory_and_receipt_guards(  # ruff: ignore[complex-structure, too-many-branches] -- each small fixture mutation exercises a different rejection
    crop_case: CropCase, case: str, message: str
) -> None:
    root = crop_case.root
    shard = root / SHARD
    receipt = root / RECEIPT
    revision = crop_case.revision
    data = json.loads(shard.read_text())
    approval = json.loads(receipt.read_text())
    if case == "missing":
        shard.unlink()
    elif case == "extra":
        write(root / "image-crops/TEST/002.yaml", data)
    elif case == "dirty":
        shard.write_text(shard.read_text() + "\n")
    elif case in {"symlink-file", "git-symlink"}:
        shard.unlink()
        shard.symlink_to(receipt)
    elif case == "symlink-dir":
        (root / "image-crops/alias").symlink_to(
            root / "image-crops/TEST", target_is_directory=True
        )
    elif case == "yml":
        shard.rename(shard.with_suffix(".yml"))
    elif case == "index":
        shard.rename(root / "image-crops/index.yaml")
    elif case == "receipt-shard":
        shard.rename(root / "image-crops/receipts/001.yaml")
    elif case == "kind":
        data["kind"] = "crop_approval"
        write(shard, data)
    elif case == "duplicate":
        write(root / "image-crops/TEST/002.yaml", data)
    elif case == "receipt-name":
        receipt.rename(root / "image-crops/receipts/other.yaml")
    elif case == "receipt-missing":
        receipt.unlink()
    elif case.startswith("member-"):
        approval["members"][0]["source_sha256" if case == "member-hash" else "top"] = (
            "b" * 64 if case == "member-hash" else 25
        )
        write(receipt, approval)
    elif case == "oversized":
        shard.write_bytes(b" " * MAX_BYTES)
    elif case == "members-duplicate":
        approval["members"].append(approval["members"][0])
        write(receipt, approval)
    else:
        assert case == "sorted"
        data["records"].append(data["records"][0] | {"card_no": "TEST-000"})
        write(shard, data)
    if case not in {"missing", "extra", "dirty", "symlink-file", "symlink-dir"}:
        revision = commit(crop_case.repo)
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        load_image_crops(root, authored_revision=revision)


@pytest.mark.parametrize(
    ("field", "value", "location"),
    [
        ("image_crop_format", True, "image_crop_format:int_type"),
        ("image_crop_format", 2, "image_crop_format:less_than_equal"),
        ("left", True, "records.0.left:int_type"),
        ("left", -1, "records.0.left:greater_than_equal"),
        ("top", "24", "records.0.top:int_type"),
        ("width", 0, "records.0.width:greater_than"),
        ("height", -1, "records.0.height:greater_than"),
        ("width", 65, "records.0:value_error"),
        ("reason", "  ", "records.0.reason:value_error"),
        ("card_no", "  ", "records.0.card_no:value_error"),
        ("region", "tw", "records.0.region:literal_error"),
        (
            "source_key",
            "img:v1:" + "a" * 64,
            "records.0.source_key:string_pattern_mismatch",
        ),
        (
            "source_sha256",
            "sha256:" + "a" * 64,
            "records.0.source_sha256:string_pattern_mismatch",
        ),
        ("receipt_id", "../receipt", "records.0.receipt_id:string_pattern_mismatch"),
        ("image_id", "img:binding:" + "a" * 64, "records.0.image_id:extra_forbidden"),
    ],
)
def test_shard_strict_fields(
    crop_template: CropCase, field: str, value: JsonValue, location: str
) -> None:
    data = json.loads((crop_template.root / SHARD).read_text())
    if field == "image_crop_format":
        data[field] = value
    else:
        data["records"][0][field] = value
    if field != "image_crop_format":
        location += "; records:too_short"
    with pytest.raises(
        ValueError,
        match="^" + re.escape("Invalid image crop fields: " + location) + "$",
    ):
        _model(CropShard, json.dumps(data).encode())


@pytest.mark.parametrize(
    ("field", "value", "location"),
    [
        ("image_crop_format", True, "image_crop_format:int_type"),
        ("approved_at", "2026-10-02T12:34:56+00:00", "approved_at:value_error"),
        ("approved_at", "2026-02-30T12:34:56Z", "approved_at:value_error"),
        ("approval_message_id", "not-a-uuid", "approval_message_id:value_error"),
        ("approval_subject", "webp_approved", "approval_subject:literal_error"),
        ("preview_sha256", "a" * 64, "preview_sha256:string_pattern_mismatch"),
        ("members", [], "members:too_short"),
    ],
)
def test_approval_strict_fields(
    crop_template: CropCase, field: str, value: JsonValue, location: str
) -> None:
    data = json.loads((crop_template.root / RECEIPT).read_text())
    data[field] = value
    with pytest.raises(
        ValueError,
        match="^" + re.escape("Invalid image crop fields: " + location) + "$",
    ):
        _model(CropApproval, json.dumps(data).encode())


@pytest.mark.parametrize(
    ("revision", "message"),
    [
        ("main", "Image crop revision must be a full Git SHA"),
        ("0" * 40, "Pinned image crop revision is unavailable"),
    ],
)
def test_bad_revision(crop_template: CropCase, revision: str, message: str) -> None:
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        load_image_crops(crop_template.root, authored_revision=revision)


def test_new_bytes_fail_closed_and_new_resources_default(
    crop_template: CropCase,
) -> None:
    crops = crop_template.load()
    descriptor = crop_template.descriptor
    changed = descriptor.model_copy(
        update={
            "raw_sha256": "sha256:" + "b" * 64,
            "id": version(descriptor.source_key, "sha256:" + "b" * 64),
        }
    )
    with pytest.raises(
        ValueError,
        match=r"^Unadopted crop source version: source_key=sha256:[a-f0-9]{64} new_hash=b{64} adopted_hashes=[a-f0-9]{64}$",
    ):
        crops.override(changed)
    unrelated = changed.model_copy(update={"source_key": "sha256:" + "c" * 64})
    assert crops.override(unrelated) is None
    for field, value in [("kind", "card"), ("provider", "other")]:
        with pytest.raises(ValueError, match=r"^Crop source must be a JP or EN image$"):
            crops.override(descriptor.model_copy(update={field: value}))
    with pytest.raises(
        ValueError, match=r"^Crop source version ID differs from adopted key$"
    ):
        crops.override(descriptor.model_copy(update={"id": "src:v1:" + "0" * 64}))


@pytest.mark.parametrize(
    ("case", "message"),
    [
        ("configuration", "Image crop configuration pin mismatch"),
        ("bool-format", "Image crop configuration pin mismatch"),
        ("missing", "Image crop dependency closure mismatch"),
        ("bytes", "Image crop dependency closure mismatch"),
        ("extra", "Image crop dependency closure mismatch"),
    ],
)
def test_f1_requires_exact_declared_closure(
    crop_template: CropCase, case: str, message: str
) -> None:
    crops = crop_template.load()
    files = crops.dependencies()
    config = crops.configuration()
    if case == "configuration":
        config["authored_revision"] = "b" * 40
    elif case == "bool-format":
        config["image_crop_format"] = True
    elif case == "missing":
        files.pop("authored/" + RECEIPT)
    elif case == "bytes":
        files["authored/" + SHARD] += b"\n"
    else:
        files["authored/image-crops/EXTRA/001.yaml"] = b"{}"
    context = BuildContext.from_inputs(
        crop_template.revision, files, {"image_crop_overrides": config}
    )
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        crops.verify_context(context)


def test_historical_unused_receipt_and_en_member_are_valid(crop_case: CropCase) -> None:
    row = record(crop_case.descriptor)
    en = row | {
        "region": "en",
        "card_no": "TEST-001EN",
        "source_key": "sha256:" + "e" * 64,
    }
    install(crop_case.root, [row, en])
    # An orphaned historical receipt is retained without making its members active.
    approval = json.loads((crop_case.root / RECEIPT).read_text())
    write(
        crop_case.root / "image-crops/receipts/history.yaml",
        approval | {"receipt_id": "history"},
    )
    revision = commit(crop_case.repo)
    crops = load_image_crops(crop_case.root, authored_revision=revision)
    assert len(crops.records) == 2
    assert len(crops.files) == 3
    assert {r.region for r in crops.records.values()} == {"jp", "en"}


@pytest.mark.parametrize(
    ("raw", "message"),
    [
        (b"image_crop_format: 1\nimage_crop_format: 1\n", "Duplicate YAML mapping key"),
        (b"left: 4.0\n", "Floating point JSON is forbidden"),
    ],
    ids=["duplicate-key", "float"],
)
def test_yaml_boundary_is_not_bypassed(raw: bytes, message: str) -> None:
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        _model(CropShard, raw)


def test_new_version_can_adopt_a_restored_default_box(crop_case: CropCase) -> None:
    old = record(crop_case.descriptor)
    checksum = "b" * 64
    new = old | {
        "source_sha256": checksum,
        "left": 6,
        "top": 15,
        "width": 64,
        "height": 48,
    }
    install(crop_case.root, [old, new])
    crops = load_image_crops(crop_case.root, authored_revision=commit(crop_case.repo))
    descriptor = crop_case.descriptor.model_copy(
        update={
            "id": version(crop_case.descriptor.source_key, "sha256:" + checksum),
            "raw_sha256": "sha256:" + checksum,
        }
    )
    restored = crops.override(descriptor)
    original = crops.override(crop_case.descriptor)
    assert restored is not None
    assert original is not None
    assert (restored.left, restored.top) == (6, 15)
    assert (original.left, original.top) == (4, 24)
