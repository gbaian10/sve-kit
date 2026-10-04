"""Targeted refusal counterexamples for the uploader's final safety boundaries."""

import gzip
from io import BytesIO
from typing import TYPE_CHECKING

import pytest
from PIL import Image

import sve_carddb.r2_upload.plan as planner
from sve_carddb.r2_upload.plan import POINTER, UploadError, plan_preview, read_member
from sve_carddb.snapshot.contract import validate
from sve_carddb.snapshot.values import canonical, digest, object_value, parse

from .r2_upload_fixtures import local as local  # ruff: ignore[useless-import-alias] -- shared immutable fixture base
from .r2_upload_fixtures import public_plan as public_plan  # ruff: ignore[useless-import-alias] -- fixture registration
from .r2_upload_fixtures import public_template as public_template  # ruff: ignore[useless-import-alias] -- fixture registration
from .r2_upload_fixtures import synthetic_transport
from .test_snapshot_preview_images import images as images  # ruff: ignore[useless-import-alias] -- synthetic image base

pytestmark = pytest.mark.usefixtures("close_sdk_clients")

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.r2_upload.plan import Plan
    from sve_carddb.snapshot.project.source import Record

    from .test_snapshot_preview_images import PublicImages


def manifest(local: Plan) -> dict[str, JsonValue]:
    key = next(m.key for m in local.members if m.phase == 2 and m.key.endswith(".json"))
    return object_value(parse(read_member(local.root, key)))


def reseal(local: Plan, value: dict[str, JsonValue]) -> None:
    for member in local.members:
        if member.phase == 2:
            (local.root / member.key).unlink()
    raw = canonical(value)
    key = "snapshots/manifests/" + digest(raw)[7:] + ".json"
    (local.root / key).write_bytes(raw)
    (local.root / (key + ".gz")).write_bytes(gzip.compress(raw, mtime=0))
    (local.root / POINTER).write_bytes(
        canonical({"manifest_path": key, "manifest_sha256": digest(raw)})
    )


def test_formal_manifest_cannot_enter_preview_uploader(local: Plan) -> None:
    value = manifest(local) | {"data_version": "20261002T010203Z-0001"}
    validate("Manifest", value)
    reseal(local, value)
    with pytest.raises(UploadError, match=r"^Public preview validation failed$"):
        plan_preview(local.root)


def test_pointer_requires_its_manifest_in_the_local_tree(local: Plan) -> None:
    hashed = "sha256:" + "f" * 64
    (local.root / POINTER).write_bytes(
        canonical(
            {
                "manifest_path": "snapshots/manifests/" + hashed[7:] + ".json",
                "manifest_sha256": hashed,
            }
        )
    )
    with pytest.raises(UploadError, match=r"^Pointed manifest is missing$"):
        plan_preview(local.root)


def test_reformatted_valid_manifest_still_requires_content_address(local: Plan) -> None:
    key = next(m.key for m in local.members if m.phase == 2 and m.key.endswith(".json"))
    raw = read_member(local.root, key) + b"\n"
    validate("Manifest", parse(raw))
    (local.root / key).write_bytes(raw)
    (local.root / (key + ".gz")).write_bytes(gzip.compress(raw, mtime=0))
    with pytest.raises(UploadError, match=r"^Content-addressed JSON hash mismatch$"):
        plan_preview(local.root)


@pytest.mark.parametrize("damage", ["pending", "unavailable", "unbound"])
def test_variants_need_an_approved_available_public_binding(
    images: PublicImages, tmp_path: Path, damage: str
) -> None:
    tables = images.tables()
    if damage == "pending":
        tables["image_asset"][0]["publication_state"] = "pending"
    elif damage == "unavailable":
        tables["image_asset"][0]["availability"] = "missing"
    else:
        tables["printing_image"] = []
    if damage == "unbound":
        root = synthetic_transport(images, tables, tmp_path / "preview")
        with pytest.raises(
            UploadError, match=r"^Image variant lacks an approved public binding$"
        ):
            plan_preview(root)
    else:
        # Reader semantics already reject these states; also guard the independent image boundary.
        with pytest.raises(
            UploadError, match=r"^Image variant lacks an approved public binding$"
        ):
            planner._image_metadata(tables, {}, set())


def test_same_size_valid_webp_substitution_cannot_keep_the_old_hash(
    images: PublicImages, tmp_path: Path
) -> None:
    tables = images.tables()
    variant = tables["image_variant"][0]
    key = str(variant["path"])
    original = (images.library / key).read_bytes()
    with Image.open(BytesIO(original)) as decoded:
        shape = decoded.size
    out = BytesIO()
    Image.new("RGB", shape, (0, 255, 0)).save(out, format="WEBP")
    replacement = out.getvalue()
    assert digest(original) != digest(replacement)
    for row in tables["image_variant"]:
        if row["path"] == key:
            row["bytes"] = len(replacement)
    root = synthetic_transport(images, tables, tmp_path / "preview", {key: replacement})
    with pytest.raises(UploadError, match=r"^Public WebP hash or size mismatch$"):
        plan_preview(root)


def test_final_text_union_comparison_guards_reader_disagreement(
    local: Plan, images: PublicImages, monkeypatch: pytest.MonkeyPatch
) -> None:
    changed = images.tables()
    changed["printing"][0]["card_no"] = "SYNTHETIC-OTHER"

    def disagrees(*_args: object, **_kwargs: object) -> dict[str, list[Record]]:
        return changed

    # Inject a reader-result fault; ordinary wire disagreement is already caught by member hashes.
    monkeypatch.setattr(planner, "read_text_all", disagrees)
    with pytest.raises(UploadError, match=r"^Text union differs from snapshot shards$"):
        plan_preview(local.root)


def test_alternative_text_member_cannot_silently_change(local: Plan) -> None:
    value = manifest(local)
    description = object_value(value["text_all"])
    old = str(description["path"])
    union = object_value(parse(read_member(local.root, old)))
    members = union["members"]
    assert isinstance(members, list)
    member = object_value(members[0])
    object_value(member["payload"])["format_version"] = "2.0.0"
    raw = canonical(union)
    key = "snapshots/blobs/" + digest(raw)[7:] + ".json"
    (local.root / old).unlink()
    (local.root / (old + ".gz")).unlink()
    (local.root / key).write_bytes(raw)
    encoded = gzip.compress(raw, mtime=0)
    (local.root / (key + ".gz")).write_bytes(encoded)
    description |= {
        "sha256": digest(raw),
        "path": key,
        "bytes": len(raw),
        "compressed_bytes": {"gzip": len(encoded), "br": None},
    }
    reseal(local, value)
    with pytest.raises(UploadError, match=r"^Public preview validation failed$"):
        plan_preview(local.root)


def test_generic_absolute_path_is_rejected_outside_image_href(local: Plan) -> None:
    reseal(local, manifest(local) | {"feedback_url": "/private-synthetic/source"})
    with pytest.raises(UploadError, match=r"^Public content contains a local path$"):
        plan_preview(local.root)
