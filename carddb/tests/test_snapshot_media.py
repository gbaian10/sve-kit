"""Synthetic 2.0 media, revision and independent wire counterexamples."""

from copy import deepcopy
from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING, cast

import pytest
from jsonschema import Draft202012Validator, ValidationError

from sve_carddb.snapshot.contract import columns, schema, validate
from sve_carddb.snapshot.export import export_snapshot
from sve_carddb.snapshot.export.measure import update
from sve_carddb.snapshot.export.wire import encode
from sve_carddb.snapshot.generate_schema import generate
from sve_carddb.snapshot.media import MAX_SAFE, image_path, image_url, prepare_media
from sve_carddb.snapshot.preview import Roots, write_preview
from sve_carddb.snapshot.preview.media_state import reservation
from sve_carddb.snapshot.profiles import MEDIA, profile
from sve_carddb.snapshot.reader import read_index, read_snapshot, read_text_all
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)

from .test_snapshot_export import BATCH
from .test_snapshot_preview_images import images as images  # ruff: ignore[useless-import-alias] -- share module-scoped synthetic WebP library

if TYPE_CHECKING:
    from pydantic import JsonValue

    from .test_snapshot_preview_images import PublicImages

GOLDEN = Path(__file__).resolve().parents[2] / "tests/fixtures/snapshot-contract/v2"


def test_independent_v2_golden_and_union() -> None:
    manifest = object_value(parse((GOLDEN / "manifest.json").read_bytes()))
    blobs = {
        string(f["key"]): canonical(
            parse((GOLDEN / "payloads" / Path(string(f["path"])).name).read_bytes())
        )
        for raw in array(manifest["files"])
        for f in (object_value(raw),)
    }
    expected = parse((GOLDEN / "expected-logical.json").read_bytes())
    assert read_snapshot(manifest, blobs) == expected
    attachments = {
        k: b for k, b in blobs.items() if k.startswith("images/") or k == "programs"
    }
    assert (
        read_text_all(
            manifest,
            canonical(parse((GOLDEN / "text-all.json").read_bytes())),
            attachments,
        )
    ) == expected
    assert len(array(object_value(expected)["printing_image"])) == 4


def test_schema_regeneration_and_frozen_column_boundaries() -> None:
    Draft202012Validator.check_schema(schema(MEDIA))
    assert (
        generate(MEDIA)
        == (
            Path(__file__).resolve().parents[1]
            / "src/sve_carddb/snapshot/schema/v2/contract.schema.json"
        ).read_bytes()
    )
    assert columns("printing_image", MEDIA) == [
        "printing_id",
        "face_id",
        "image_id",
        "publication_state",
        "availability",
        "withdrawal_reason",
        "card_version",
        "art_version",
        "variants",
    ]
    assert columns("printing_image") == ["printing_id", "face_id", "image_id"]
    assert "path" in columns("image_variant")
    assert "path" not in columns("image_variant", MEDIA)
    assert profile(MEDIA).width("images", "detail", "home_set", "BP01") == 32
    assert profile(MEDIA).width("images", "detail", "global", "") == 1


def test_shared_image_url_vectors() -> None:
    for raw in array(parse((GOLDEN / "image-url-cases.json").read_bytes())):
        case = object_value(raw)
        if "reject" in case:
            continue
        assert (
            image_url(
                integer(case["int_id"]),
                integer(case["ordinal"]),
                string(case["size"]),
                integer(case["version"]),
            )
            == case["url"]
        )
        assert "?" not in image_path(
            integer(case["int_id"]), integer(case["ordinal"]), string(case["size"])
        )


@pytest.mark.parametrize("field", ["int_id", "ordinal", "version"])
@pytest.mark.parametrize("bad", [-1, MAX_SAFE + 1, True, 1.5, "01"])
def test_url_rejects_unsafe_integer(field: str, bad: JsonValue) -> None:
    values: dict[str, JsonValue] = {"int_id": 1, "ordinal": 0, "version": 1}
    values[field] = bad
    # Cast only the deliberately invalid caller boundary, not validated JSON.
    with pytest.raises(ValueError, match=r"^Image URL integer outside safe domain$"):
        image_url(
            cast("int", values["int_id"]),
            cast("int", values["ordinal"]),
            "card_s",
            cast("int", values["version"]),
        )


@pytest.mark.parametrize("field", ["int_id", "version"])
def test_url_positive_domain(field: str) -> None:
    with pytest.raises(ValueError, match=r"^Image URL integer outside safe domain$"):
        image_url(
            0 if field == "int_id" else 1, 0, "card_s", 0 if field == "version" else 1
        )


def test_unknown_size() -> None:
    with pytest.raises(ValueError, match=r"^Unknown image size$"):
        image_url(1, 0, "png", 1)


def test_project_export_and_preview(images: PublicImages, tmp_path: Path) -> None:
    plan = prepare_media(images.projection, images.library, revision=7)
    snapshot = export_snapshot(
        plan.projection, images.ownership, BATCH, format_version=MEDIA
    )
    snapshot.verify(plan.projection)
    media = plan.projection.tables["printing_image"][0]
    assert (media["card_version"], media["art_version"]) == (7, 7)
    assert media["variants"] == [
        {k: r[k] for k in ("size_key", "width", "height")}
        for r in images.projection.tables["image_variant"]
    ]
    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    report = write_preview(
        snapshot, roots, {}, image_source=images.library, media_plan=plan
    )
    assert report["images"] == {
        "unique_files": 5,
        "unique_bytes": sum(integer(a["bytes"]) for a in plan.assets),
    }
    assert {
        p.relative_to(roots.preview).as_posix() for p in roots.preview.rglob("*.webp")
    } == {string(a["path"]) for a in plan.assets}
    assert not (roots.preview / "images/sha256").exists()
    assert (
        parse((roots.preview / "private/media-committed.json").read_bytes())
        == plan.state
    )
    files = {
        string(f["key"]): f
        for r in array(snapshot.manifest["files"])
        for f in (object_value(r),)
    }
    deps = [
        array(f["dependencies"])
        for f in files.values()
        if f["role"] == "images" and "/home_set/" in string(f["key"])
    ]
    assert deps
    assert all(len(d) >= 2 for d in deps)
    assert all(
        not string(object_value(d)["key"]).startswith("images/")
        for refs in deps
        for d in refs
    )


def test_text_change_keeps_versions_and_same_payloads(images: PublicImages) -> None:
    first = prepare_media(images.projection, images.library, revision=7)
    text = deepcopy(images.projection)
    text.tables["printing"][0]["rarity_raw"] = "Synthetic replacement rarity"
    later = prepare_media(text, images.library, revision=8, previous=first.state)
    assert (
        later.projection.tables["printing_image"]
        == first.projection.tables["printing_image"]
    )
    a = export_snapshot(first.projection, images.ownership, BATCH, format_version=MEDIA)
    b = export_snapshot(later.projection, images.ownership, BATCH, format_version=MEDIA)
    changed = array(update(a, b)["changed_keys"])
    # Bootstrap hashes are exact media dependencies, so changed bases reseal media JSON;
    # their displayed image versions and all five target bytes remain unchanged.
    assert first.assets == later.assets
    assert changed


@pytest.mark.parametrize(
    "missing", ["missing", "unfetched", "pending", "withdrawn", "removed"]
)
def test_restoration_uses_new_event_revision(
    images: PublicImages, missing: str
) -> None:
    first = prepare_media(images.projection, images.library, revision=7)
    absent = deepcopy(images.projection)
    absent.tables["image_variant"] = []
    if missing == "removed":
        absent.tables["printing_image"] = []
    elif missing in {"pending", "withdrawn"}:
        absent.tables["image_asset"][0]["publication_state"] = missing
        if missing == "withdrawn":
            absent.tables["image_asset"][0]["withdrawal_reason"] = (
                "Synthetic withdrawal"
            )
    else:
        absent.tables["image_asset"][0]["availability"] = missing
    second = prepare_media(absent, None, revision=8, previous=first.state)
    for row in second.projection.tables["printing_image"]:
        assert row["card_version"] is None
        assert row["art_version"] is None
        assert row["variants"] == []
    export_snapshot(second.projection, images.ownership, BATCH, format_version=MEDIA)
    restored = prepare_media(
        images.projection, images.library, revision=9, previous=second.state
    )
    assert restored.projection.tables["printing_image"][0]["card_version"] == 9
    assert restored.projection.tables["printing_image"][0]["art_version"] == 9


@pytest.mark.parametrize("group", ["card", "art"])
def test_binding_and_a_b_a_never_reuse_token(
    images: PublicImages, group: str, tmp_path: Path
) -> None:
    import shutil  # ruff: ignore[import-outside-top-level] -- independent private test cache
    from io import BytesIO  # ruff: ignore[import-outside-top-level] -- synthetic bytes only

    from PIL import Image  # ruff: ignore[import-outside-top-level] -- synthesize a changed output of the same dimensions

    shutil.copytree(images.library, tmp_path / "library")
    first = prepare_media(images.projection, images.library, revision=7)
    changed = deepcopy(images.projection)
    row = next(
        r for r in changed.tables["image_variant"] if r["size_key"] == group + "_s"
    )
    raw = BytesIO()
    Image.new(
        "RGB", (integer(row["width"]), integer(row["height"])), (90, 40, 170)
    ).save(raw, format="WEBP")
    content = raw.getvalue()
    hashed = digest(content)[7:]
    row.update(
        path="images/sha256/" + hashed[:2] + "/" + hashed + ".webp", bytes=len(content)
    )
    dest = tmp_path / "library" / string(row["path"])
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(content)
    second = prepare_media(
        changed, tmp_path / "library", revision=8, previous=first.state
    )
    media = second.projection.tables["printing_image"][0]
    assert media[group + "_version"] == 8
    assert media[("art" if group == "card" else "card") + "_version"] == 7
    restored = prepare_media(
        images.projection, images.library, revision=9, previous=second.state
    )
    assert restored.projection.tables["printing_image"][0][group + "_version"] == 9
    rebound = deepcopy(images.projection)
    rebound.tables["image_asset"][0]["id"] = "new:image"
    for table in ("image_variant", "printing_image"):
        for r in rebound.tables[table]:
            r["image_id"] = "new:image"
    binding = prepare_media(
        rebound, images.library, revision=10, previous=restored.state
    )
    assert binding.projection.tables["printing_image"][0]["card_version"] == 10
    assert binding.projection.tables["printing_image"][0]["art_version"] == 10


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("status", "Media status differs from source image"),
        ("version", "Available media requires both image versions"),
        ("missing_size", "Available media requires sorted five display variants"),
        ("duplicate_size", "Available media requires sorted five display variants"),
        ("size_order", "Available media requires sorted five display variants"),
        ("dimensions", "Media dimensions differ from image variant"),
    ],
)
def test_independent_media_counterexamples(
    images: PublicImages, change: str, message: str
) -> None:
    plan = prepare_media(images.projection, images.library, revision=7)
    row = plan.projection.tables["printing_image"][0]
    if change == "status":
        row["availability"] = "missing"
    elif change == "version":
        row["card_version"] = None
    elif change == "missing_size":
        array(row["variants"]).pop()
    elif change == "duplicate_size":
        array(row["variants"]).append(array(row["variants"])[-1])
    elif change == "size_order":
        array(row["variants"]).reverse()
    else:
        object_value(array(row["variants"])[0])["width"] = 123
    with pytest.raises(ValueError, match=r"^" + message + "$"):
        export_snapshot(plan.projection, images.ownership, BATCH, format_version=MEDIA)


@pytest.mark.parametrize("version", [0, MAX_SAFE + 1, True])
def test_wire_token_domain(images: PublicImages, version: JsonValue) -> None:
    plan = prepare_media(images.projection, images.library, revision=7)
    row = plan.projection.tables["printing_image"][0]
    row["art_version"] = version
    if version == MAX_SAFE + 1:
        with pytest.raises(ValueError, match=r"^Expected safe integer$"):
            validate("printing_image", encode("printing_image", row, MEDIA), MEDIA)
        return
    with pytest.raises(ValidationError) as exc:
        validate("printing_image", encode("printing_image", row, MEDIA), MEDIA)
    assert exc.value.validator in {"anyOf", "minimum", "maximum", "type"}


def test_failed_reservation_is_not_reused(tmp_path: Path) -> None:
    roots = Roots(tmp_path / "preview", tmp_path / "formal")

    def interrupt() -> None:
        with reservation(roots) as (revision, previous):
            assert revision == 1
            assert previous is None
            raise RuntimeError("synthetic interruption")

    with pytest.raises(RuntimeError, match=r"^synthetic interruption$"):
        interrupt()
    with reservation(roots) as (revision, previous):
        assert revision == 2
        assert previous is None
    with reservation(roots) as (revision, _):
        assert revision == 3


def test_revision_cannot_reuse_committed_number(images: PublicImages) -> None:
    plan = prepare_media(images.projection, images.library, revision=7)
    with pytest.raises(
        ValueError, match=r"^Media revision must advance the committed state$"
    ):
        prepare_media(
            images.projection, images.library, revision=7, previous=plan.state
        )


def test_v2_writer_requires_matching_plan(images: PublicImages, tmp_path: Path) -> None:
    plan = prepare_media(images.projection, images.library, revision=7)
    snapshot = export_snapshot(
        plan.projection, images.ownership, BATCH, format_version=MEDIA
    )
    with pytest.raises(
        ValueError, match=r"^Preview requires the matching verified media plan$"
    ):
        write_preview(snapshot, Roots(tmp_path / "preview", tmp_path / "formal"), {})


def test_index_two_window_and_no_recursive_changes_history() -> None:
    manifest = object_value(parse((GOLDEN / "manifest.json").read_bytes()))

    def entry(value: dict[str, JsonValue]) -> tuple[dict[str, JsonValue], bytes]:
        value["data_version"] = string(value["data_version"]).replace("preview-", "")
        raw = canonical(value)
        h = digest(raw)
        return {
            k: value[k]
            for k in (
                "data_version",
                "published_at",
                "format_version",
                "min_reader_version",
                "required_capabilities",
                "engine_support_target",
            )
        } | {
            "manifest_path": "snapshots/manifests/" + h[7:] + ".json",
            "manifest_sha256": h,
        }, raw

    e, raw = entry(manifest)
    index: dict[str, JsonValue] = {
        "index_format": 2,
        "revision": 1,
        "current": e,
        "previous": None,
    }
    assert read_index(index, {string(e["manifest_sha256"]): raw}) == index
    duplicate = index | {"previous": e}
    with pytest.raises(
        ValueError, match=r"^Index current and previous must be distinct releases$"
    ):
        read_index(duplicate, {string(e["manifest_sha256"]): raw})
    with pytest.raises(
        ValueError, match=r"^Index manifest set must equal readable window$"
    ):
        read_index(index, {})
    with pytest.raises(ValidationError):
        validate("Index", index | {"pages": []}, MEDIA)


def _changed_payload(
    snapshot: object, *, add_dependency: bool
) -> tuple[dict[str, JsonValue], dict[str, bytes]]:
    from sve_carddb.snapshot.export import Snapshot  # ruff: ignore[import-outside-top-level] -- explicit synthetic helper type boundary

    assert isinstance(snapshot, Snapshot)
    manifest = deepcopy(snapshot.manifest)
    files = {
        string(f["key"]): f
        for r in array(manifest["files"])
        for f in (object_value(r),)
    }
    key = next(k for k in files if k.startswith("images/detail/home_set/"))
    if add_dependency:
        extra = next(k for k in files if k.startswith("images/detail/global/"))
        array(files[key]["dependencies"]).append(
            {"key": extra, "sha256": files[extra]["sha256"]}
        )
        array(files[key]["dependencies"]).sort(
            key=lambda r: string(object_value(r)["key"])
        )
    else:
        files[key]["dependencies"] = [
            r
            for r in array(files[key]["dependencies"])
            if object_value(r)["key"] == "config"
        ]
    return manifest, {k: b.raw for k, b in snapshot.payloads.items()}


@pytest.mark.parametrize("extra", [True, False])
def test_media_rejects_missing_or_extra_bootstrap_dependencies(
    images: PublicImages, extra: bool
) -> None:
    plan = prepare_media(images.projection, images.library, revision=7)
    snapshot = export_snapshot(
        plan.projection, images.ownership, BATCH, format_version=MEDIA
    )
    manifest, blobs = _changed_payload(snapshot, add_dependency=extra)
    with pytest.raises(
        ValueError, match=r"^Media dependencies must equal exact bootstrap closure$"
    ):
        read_snapshot(manifest, blobs)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("face_id", "face"),
        ("digital_phase", "normal"),
        ("effect_similarity", "reworked"),
        ("review_level", "confirmed"),
        ("review_level", "sampled"),
    ],
)
def test_same_name_is_card_level_not_human_review(
    images: PublicImages, field: str, value: JsonValue
) -> None:
    projection = deepcopy(images.projection)
    row = projection.tables["digital_link"][0]
    row.update(
        relation="same_name",
        face_id=None,
        digital_phase=None,
        effect_similarity=None,
        review_level="unreviewed",
    )
    row[field] = value
    plan = prepare_media(projection, images.library, revision=7)
    with pytest.raises(
        ValueError, match=r"^Same-name browsing must be unreviewed and card-level$"
    ):
        export_snapshot(plan.projection, images.ownership, BATCH, format_version=MEDIA)


def test_two_printings_and_permanent_back_ordinal_share_source_not_target(
    images: PublicImages,
) -> None:
    projection = deepcopy(images.projection)
    original = projection.tables["printing"][0]
    another = deepcopy(original)
    another.update(
        id="printing:another", int_id=345, card_no="SYN-EN001aⓈ", region="jp"
    )
    projection.tables["printing"].append(another)
    back = deepcopy(projection.tables["face"][0])
    back.update(id="face:back", ordinal=7, side="back", current=[], wording=[])
    projection.tables["face"].append(back)
    array(projection.tables["card"][0]["faces"]).append("face:back")
    back_printing = deepcopy(object_value(array(original["faces"])[0]))
    back_printing["face_id"] = "face:back"
    # Unknown back text has no adopted observation; the image uses permanent ordinal 7.
    back_printing["observations"] = []
    array(another["faces"]).append(back_printing)
    projection.tables["printing_image"].extend(
        [
            {"printing_id": "printing:another", "face_id": "face", "image_id": "image"},
            {
                "printing_id": "printing:another",
                "face_id": "face:back",
                "image_id": "image",
            },
        ]
    )
    plan = prepare_media(projection, images.library, revision=7)
    paths = {string(a["path"]) for a in plan.assets}
    assert len(paths) == 15
    assert "images/art_m/345-f7.webp" in paths
    assert "images/card_s/345.webp" in paths
    assert "images/card_s/345-f1.webp" not in paths
    assert len({string(a["sha256"]) for a in plan.assets}) == 3


def test_byte_change_after_sealing_rejected_before_activation(
    images: PublicImages, tmp_path: Path
) -> None:
    import shutil  # ruff: ignore[import-outside-top-level] -- isolate a mutable synthetic cache

    library = tmp_path / "library"
    shutil.copytree(images.library, library)
    plan = prepare_media(images.projection, library, revision=7)
    asset = plan.assets[0]
    source = library / string(asset["source"])
    content = source.read_bytes()
    source.write_bytes(bytes([content[0] ^ 1]) + content[1:])
    snapshot = export_snapshot(
        plan.projection, images.ownership, BATCH, format_version=MEDIA
    )
    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    with pytest.raises(ValueError, match=r"^Media plan source hash or bytes mismatch$"):
        write_preview(snapshot, roots, {}, image_source=library, media_plan=plan)
    assert not (roots.preview / "snapshots/preview/current.json").exists()


def test_media_uses_printing_home_not_card_home(images: PublicImages) -> None:
    from sve_carddb.snapshot.export import Ownership  # ruff: ignore[import-outside-top-level] -- independent build-only ownership input

    projection = deepcopy(images.projection)
    family = deepcopy(projection.tables["product_family"][0])
    family["id"] = "new:home"
    projection.tables["product_family"].append(family)
    homes = {string(p["id"]): "new:home" for p in projection.tables["printing"]}
    plan = prepare_media(projection, images.library, revision=7)
    snapshot = export_snapshot(
        plan.projection, Ownership(homes), BATCH, format_version=MEDIA
    )
    assert any(
        string(object_value(f)["key"]).startswith("images/detail/home_set/new%3Ahome/")
        for f in array(snapshot.manifest["files"])
    )


@pytest.mark.parametrize("field", ["card_version", "art_version", "variants"])
def test_unavailable_media_never_offers_url(images: PublicImages, field: str) -> None:
    plan = prepare_media(deepcopy(images.projection), images.library, revision=7)
    for row in plan.projection.tables["image_asset"]:
        row["availability"] = "missing"
    plan.projection.tables["image_variant"] = []
    media = plan.projection.tables["printing_image"][0]
    media.update(
        availability="missing", card_version=None, art_version=None, variants=[]
    )
    media[field] = (
        [{"size_key": "art_s", "width": 160, "height": 120}]
        if field == "variants"
        else 7
    )
    with pytest.raises(
        ValueError, match=r"^Unavailable media cannot provide image URLs$"
    ):
        export_snapshot(plan.projection, images.ownership, BATCH, format_version=MEDIA)


@pytest.mark.parametrize("missing", ["card_s", "art_s"])
def test_prepare_requires_complete_verified_outputs(
    images: PublicImages, missing: str
) -> None:
    projection = deepcopy(images.projection)
    projection.tables["image_variant"] = [
        r for r in projection.tables["image_variant"] if r["size_key"] != missing
    ]
    with pytest.raises(
        ValueError, match=r"^Available preview printing image requires all five sizes$"
    ):
        prepare_media(projection, images.library, revision=7)


def test_local_reservation_fsync_before_yield(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    calls: list[int] = []
    monkeypatch.setattr(
        "sve_carddb.snapshot.preview.media_state.os.fsync", calls.append
    )
    with reservation(Roots(tmp_path / "preview", tmp_path / "formal")) as (revision, _):
        assert revision == 1
        assert calls


def test_same_name_capability_nonempty_and_old_profile_rejects(
    images: PublicImages,
) -> None:
    projection = deepcopy(images.projection)
    row = projection.tables["digital_link"][0]
    row.update(
        relation="same_name",
        face_id=None,
        digital_phase=None,
        effect_similarity=None,
        review_level="unreviewed",
    )
    projection.tables["card_voice"] = []
    plan = prepare_media(projection, images.library, revision=7)
    snapshot = export_snapshot(
        plan.projection, images.ownership, BATCH, format_version=MEDIA
    )
    assert "digital-same-name-links-v1" in array(
        snapshot.manifest["required_capabilities"]
    )
    assert (
        read_snapshot(
            snapshot.manifest, {k: b.raw for k, b in snapshot.payloads.items()}
        )["digital_link"][0]["review_level"]
        == "unreviewed"
    )
    with pytest.raises(ValidationError):
        validate("digital_link", encode("digital_link", row), "1.1.0")


def test_failed_image_group_does_not_switch_pointer(
    images: PublicImages, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    import sve_carddb.snapshot.preview as writer  # ruff: ignore[import-outside-top-level] -- fault only the output writer boundary

    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    plan = prepare_media(images.projection, images.library, revision=7)
    snapshot = export_snapshot(
        plan.projection, images.ownership, BATCH, format_version=MEDIA
    )
    write_preview(snapshot, roots, {}, image_source=images.library, media_plan=plan)
    pointer = (roots.preview / "snapshots/preview/current.json").read_bytes()
    observed = writer._write
    count = 0

    def interrupted(roots: Roots, path: str, raw: bytes, *, immutable: bool) -> None:
        nonlocal count
        if path.startswith("images/"):
            count += 1
            if count == 3:
                raise RuntimeError("synthetic interrupted group")
        observed(roots, path, raw, immutable=immutable)

    monkeypatch.setattr(writer, "_write", interrupted)
    with pytest.raises(RuntimeError, match=r"^synthetic interrupted group$"):
        write_preview(snapshot, roots, {}, image_source=images.library, media_plan=plan)
    assert (roots.preview / "snapshots/preview/current.json").read_bytes() == pointer
    assert (
        parse((roots.preview / "private/media-committed.json").read_bytes())
        == plan.state
    )


@pytest.mark.parametrize(("width", "height"), [(128, 96), (12, 16)])
def test_landscape_and_tiny_images_expose_actual_dimensions(
    images: PublicImages, tmp_path: Path, width: int, height: int
) -> None:
    from sve_carddb.image_variants import build_variants  # ruff: ignore[import-outside-top-level] -- reuse the production transform for synthetic inputs

    from .test_image_variants import png, source  # ruff: ignore[import-outside-top-level] -- generated pixels, no real cards

    result = build_variants(
        source(png(width, height)),
        blob_root=tmp_path / "library",
        cache_root=tmp_path / "cache",
    )
    projection = deepcopy(images.projection)
    projection.tables["image_asset"][0].update(width=width, height=height)
    projection.tables["image_variant"] = [
        {
            "image_id": "image",
            "size_key": v.size_key,
            "format": v.format,
            "path": v.path,
            "width": v.width,
            "height": v.height,
            "bytes": v.bytes,
        }
        for v in sorted(result.variants, key=lambda v: v.size_key)
    ]
    plan = prepare_media(projection, tmp_path / "library", revision=7)
    display = {
        string(object_value(v)["size_key"]): object_value(v)
        for v in array(plan.projection.tables["printing_image"][0]["variants"])
    }
    assert display["card_l"]["width"] == width
    assert display["card_l"]["height"] == height
    export_snapshot(plan.projection, images.ownership, BATCH, format_version=MEDIA)


@pytest.mark.parametrize("field", ["width", "bytes"])
def test_media_source_decoded_dimensions_and_length_are_verified(
    images: PublicImages, field: str
) -> None:
    projection = deepcopy(images.projection)
    variant = next(
        r for r in projection.tables["image_variant"] if r["size_key"] == "card_s"
    )
    variant[field] = integer(variant[field]) + 1
    message = (
        "Preview image decoded format or dimensions mismatch"
        if field == "width"
        else "Preview image hash or bytes mismatch"
    )
    with pytest.raises(ValueError, match="^" + message + "$"):
        prepare_media(projection, images.library, revision=7)


@pytest.mark.parametrize(
    "change",
    ["revision", "fields", "fingerprint", "token", "binding", "active", "empty"],
)
def test_committed_state_rejects_corruption(images: PublicImages, change: str) -> None:
    plan = prepare_media(images.projection, images.library, revision=7)
    previous = deepcopy(plan.state)
    member = next(iter(object_value(previous["members"]).values()))
    row = object_value(member)
    if change == "revision":
        previous["revision"] = 0
    elif change == "fields":
        row["unknown"] = True
    elif change == "fingerprint":
        row["card"] = "sha256:short"
    elif change == "token":
        row["card_version"] = 8
    elif change == "binding":
        array(row["binding"])[1] = True
    elif change == "active":
        row["active"] = False
    else:
        previous = {}
    with pytest.raises(ValueError, match=r"^Invalid committed media state$"):
        prepare_media(images.projection, images.library, revision=8, previous=previous)


def test_group_version_selector_and_disabled_urls(images: PublicImages) -> None:
    from sve_carddb.snapshot.media import display_url  # ruff: ignore[import-outside-top-level] -- test the visible-row accessor without a producer

    plan = prepare_media(images.projection, images.library, revision=7)
    media = plan.projection.tables["printing_image"][0]
    media["art_version"] = 11
    printing, face = (
        images.projection.tables["printing"][0],
        images.projection.tables["face"][0],
    )
    assert string(display_url(printing, face, media, "card_s")).endswith("?v=7")
    assert string(display_url(printing, face, media, "art_m")).endswith("?v=11")
    media["availability"] = "missing"
    assert display_url(printing, face, media, "card_s") is None
    with pytest.raises(
        ValueError, match=r"^Image URL media belongs to another printing face$"
    ):
        display_url(printing | {"id": "other"}, face, media, "card_s")


def test_retry_cannot_change_reserved_outputs(images: PublicImages) -> None:
    plan = prepare_media(images.projection, images.library, revision=7)
    same = prepare_media(images.projection, images.library, revision=7)
    plan.verify_retry(same)
    different = replace(same, assets=tuple(deepcopy(same.assets)))
    different.assets[0]["sha256"] = "sha256:" + "0" * 64
    with pytest.raises(ValueError, match=r"^Retry changes reserved media plan$"):
        plan.verify_retry(different)


def test_first_image_metadata_avoids_global_source_detail(images: PublicImages) -> None:
    from sve_carddb.snapshot.export.page_cost import page_image_cost  # ruff: ignore[import-outside-top-level] -- inspect the public metadata transfer model

    plan = prepare_media(images.projection, images.library, revision=7)
    result = export_snapshot(
        plan.projection, images.ownership, BATCH, format_version=MEDIA
    )
    report = page_image_cost(result)
    assert report["source_details_required"] is False
    assert object_value(object_value(report["cold"])["requests"])["max"] == 1


@pytest.mark.parametrize(
    "incompatible", ["future_format", "unknown_capability", "future_minimum"]
)
def test_index_future_current_is_not_corruption_and_compatible_previous_is_readable(
    incompatible: str,
) -> None:
    from sve_carddb.snapshot.reader import select_index_entry  # ruff: ignore[import-outside-top-level] -- neutral index negotiation boundary

    manifest = object_value(parse((GOLDEN / "manifest.json").read_bytes()))
    manifest["data_version"] = string(manifest["data_version"]).removeprefix("preview-")
    raw = canonical(manifest)
    hashed = digest(raw)
    previous: dict[str, JsonValue] = {
        f: manifest[f]
        for f in (
            "data_version",
            "published_at",
            "format_version",
            "min_reader_version",
            "required_capabilities",
            "engine_support_target",
        )
    } | {
        "manifest_path": "snapshots/manifests/" + hashed[7:] + ".json",
        "manifest_sha256": hashed,
    }
    current = deepcopy(previous)
    current.update(
        data_version="20261005T010203Z-0001",
        manifest_path="snapshots/manifests/" + "a" * 64 + ".json",
        manifest_sha256="sha256:" + "a" * 64,
    )
    if incompatible == "future_format":
        current.update(format_version="3.0.0", min_reader_version="3.0.0")
    elif incompatible == "future_minimum":
        current["min_reader_version"] = "3.0.0"
    else:
        capabilities: list[JsonValue] = [
            *array(current["required_capabilities"]),
            "future-feature-v1",
        ]
        current["required_capabilities"] = sorted(capabilities, key=string)
    index: dict[str, JsonValue] = {
        "index_format": 2,
        "revision": 3,
        "current": current,
        "previous": previous,
    }
    assert read_index(index, {hashed: raw}) == index
    assert select_index_entry(index) == previous
    no_compatible = index | {"previous": None}
    assert read_index(no_compatible, {}) == no_compatible
    assert select_index_entry(no_compatible) is None


@pytest.mark.parametrize("failure", ["endpoint", "duplicate", "human_shadow"])
def test_nonempty_same_name_requires_endpoints_and_excludes_extra_authority(
    images: PublicImages, failure: str
) -> None:
    projection = deepcopy(images.projection)
    rule = projection.tables["digital_link"][0]
    rule.update(
        relation="same_name",
        face_id=None,
        digital_phase=None,
        effect_similarity=None,
        review_level="unreviewed",
    )
    projection.tables["card_voice"] = []
    if failure in {"duplicate", "human_shadow"}:
        other = deepcopy(rule)
        other["id"] = "digital-link:second"
        if failure == "human_shadow":
            other.update(relation="same_character", review_level="confirmed")
        projection.tables["digital_link"].append(other)
    plan = prepare_media(projection, images.library, revision=7)
    if failure == "endpoint":
        array(plan.projection.config["digital_endpoints"]).pop()
    message = {
        "endpoint": "Media config requires both sorted game endpoints",
        "duplicate": "Duplicate or human-shadowed same-name card pair",
        "human_shadow": "Duplicate or human-shadowed same-name card pair",
    }[failure]
    with pytest.raises(ValueError, match="^" + message + "$"):
        export_snapshot(plan.projection, images.ownership, BATCH, format_version=MEDIA)


def test_v2_offline_endpoints_are_present_without_policy_links(
    images: PublicImages,
) -> None:
    projection = deepcopy(images.projection)
    projection.config["digital_endpoints"] = []
    projection.tables["digital_link"] = []
    projection.tables["card_voice"] = []
    plan = prepare_media(projection, images.library, revision=7)
    snapshot = export_snapshot(
        plan.projection, images.ownership, BATCH, format_version=MEDIA
    )
    config = object_value(parse(snapshot.payloads["config"].raw))
    endpoints = [object_value(e) for e in array(config["digital_endpoints"])]
    assert [e["game"] for e in endpoints] == ["sv1", "svwb"]
    assert [e["status"] for e in endpoints] == ["unknown", "unknown"]
    assert [e["refresh_policy"] for e in endpoints] == ["frozen", "on_sve_release"]
    assert [object_value(e["language_map"])["zh-Hant"] for e in endpoints] == [
        "zh-tw",
        "cht",
    ]
    assert (
        endpoints[0]["card_url_template"]
        == "https://shadowverse-portal.com/card/{official_id}?lang={provider_lang}"
    )
    assert (
        endpoints[1]["card_url_template"]
        == "https://shadowverse-wb.com/{provider_lang}/deck/cardslist/card/?card_id={official_id}"
    )
    assert plan.projection.tables["digital_link"] == []
    assert projection.config["digital_endpoints"] == []


@pytest.mark.parametrize(
    "failure", ["empty", "missing", "duplicate", "reverse", "refresh"]
)
def test_v2_reader_rejects_endpoint_config_even_without_links(
    images: PublicImages, failure: str
) -> None:
    from sve_carddb.snapshot.reader_media import validate_digital  # ruff: ignore[import-outside-top-level] -- independently exercise the reader without exporter rewriting

    plan = prepare_media(images.projection, images.library, revision=7)
    config = deepcopy(plan.projection.config)
    endpoints = array(config["digital_endpoints"])
    if failure == "empty":
        endpoints.clear()
    elif failure == "missing":
        endpoints.pop()
    elif failure == "duplicate":
        endpoints.append(deepcopy(endpoints[0]))
    elif failure == "reverse":
        endpoints.reverse()
    else:
        object_value(endpoints[1])["refresh_policy"] = "frozen"
    message = (
        "Digital endpoint refresh policy differs from game"
        if failure == "refresh"
        else "Media config requires both sorted game endpoints"
    )
    view = plan.projection.tables | {"digital_card": [], "digital_link": []}
    with pytest.raises(ValueError, match="^" + message + "$"):
        validate_digital(view, config)
    with pytest.raises(ValueError, match="^" + message + "$"):
        export_snapshot(
            replace(plan.projection, config=config),
            images.ownership,
            BATCH,
            format_version=MEDIA,
        )


def test_prepare_rejects_duplicate_permanent_path(images: PublicImages) -> None:
    projection = deepcopy(images.projection)
    another = deepcopy(projection.tables["printing"][0])
    another["id"] = "printing:collision"
    projection.tables["printing"].append(another)
    projection.tables["printing_image"].append(
        projection.tables["printing_image"][0] | {"printing_id": "printing:collision"}
    )
    with pytest.raises(ValueError, match=r"^Duplicate permanent image path$"):
        prepare_media(projection, images.library, revision=7)


def test_prepare_rejects_face_absent_from_printing(images: PublicImages) -> None:
    projection = deepcopy(images.projection)
    projection.tables["printing"][0]["faces"] = []
    with pytest.raises(ValueError, match=r"^Media face absent from printing$"):
        prepare_media(projection, images.library, revision=7)


def test_legacy_writer_rejects_media_plan(images: PublicImages, tmp_path: Path) -> None:
    plan = prepare_media(images.projection, images.library, revision=7)
    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    with pytest.raises(
        ValueError, match=r"^Legacy preview cannot consume a media plan$"
    ):
        write_preview(
            images.snapshot(), roots, {}, image_source=images.library, media_plan=plan
        )
    assert not (roots.preview / "snapshots/preview/current.json").exists()


@pytest.mark.parametrize("revisions", [[1, 3], [1, 1]])
def test_reservation_rejects_nonmonotonic_journal(
    tmp_path: Path, revisions: list[int]
) -> None:
    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    journal = roots.destination("private/media-revisions.jsonl")
    journal.parent.mkdir(parents=True)
    raw = b"".join(canonical({"revision": r}) + b"\n" for r in revisions)
    journal.write_bytes(raw)
    with (
        pytest.raises(
            ValueError, match=r"^Preview media reservation journal is not monotonic$"
        ),
        reservation(roots),
    ):
        pytest.fail("Corrupt journal must not yield a reservation")
    assert journal.read_bytes() == raw


def test_written_media_corruption_does_not_activate(
    images: PublicImages, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    import sve_carddb.snapshot.preview as writer  # ruff: ignore[import-outside-top-level] -- corrupt only the sealed destination at the activation boundary

    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    old = prepare_media(images.projection, images.library, revision=7)
    old_snapshot = export_snapshot(
        old.projection, images.ownership, BATCH, format_version=MEDIA
    )
    write_preview(old_snapshot, roots, {}, image_source=images.library, media_plan=old)
    pointer_path = roots.preview / "snapshots/preview/current.json"
    pointer = pointer_path.read_bytes()
    projection = deepcopy(images.projection)
    projection.tables["printing"][0]["rarity_raw"] = "Synthetic changed metadata"
    new = prepare_media(projection, images.library, revision=8, previous=old.state)
    snapshot = export_snapshot(
        new.projection, images.ownership, BATCH, format_version=MEDIA
    )
    assert digest(canonical(snapshot.manifest)) != digest(
        canonical(old_snapshot.manifest)
    )
    original = writer._write
    corrupted = False

    def corrupt_after_sealing(
        roots: Roots, path: str, raw: bytes, *, immutable: bool
    ) -> None:
        nonlocal corrupted
        original(roots, path, raw, immutable=immutable)
        if path.startswith("reports/"):
            assert pointer_path.read_bytes() == pointer
            assert all(
                roots.destination(string(a["path"])).exists() for a in new.assets
            )
            target = roots.destination(string(new.assets[0]["path"]))
            content = target.read_bytes()
            target.write_bytes(bytes([content[0] ^ 1]) + content[1:])
            corrupted = True

    monkeypatch.setattr(writer, "_write", corrupt_after_sealing)
    with pytest.raises(ValueError, match=r"^Written media differs from sealed plan$"):
        write_preview(snapshot, roots, {}, image_source=images.library, media_plan=new)
    assert corrupted
    assert pointer_path.read_bytes() == pointer
    assert (
        parse((roots.preview / "private/media-committed.json").read_bytes())
        == old.state
    )
