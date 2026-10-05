"""Synthetic minor-profile, stable-band and independent wire acceptance tests."""

from copy import deepcopy
from dataclasses import replace
from itertools import starmap
from pathlib import Path
from typing import TYPE_CHECKING
from zlib import compress as synthetic_compress

import pytest
from jsonschema import ValidationError
from pydantic import JsonValue

from sve_carddb.snapshot.contract import validate
from sve_carddb.snapshot.export import Brotli, Ownership, Snapshot, export_snapshot
from sve_carddb.snapshot.export.layout import Group, Layout
from sve_carddb.snapshot.export.measure import measure, update
from sve_carddb.snapshot.profiles import MEDIA, profile
from sve_carddb.snapshot.project import Projection
from sve_carddb.snapshot.reader import read_snapshot, read_text_all
from sve_carddb.snapshot.values import (
    array,
    bucket,
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)

from .snapshot_contract_fixtures import fixture
from .test_snapshot_export import BATCH, cloned
from .test_snapshot_export import logical as logical  # ruff: ignore[useless-import-alias] -- reuse expensive module setup
from .test_snapshot_wording import pending_view

if TYPE_CHECKING:
    from collections.abc import Iterator

GOLDEN = Path(__file__).resolve().parents[2] / "tests/fixtures/snapshot-contract/v2"
CODEC = Brotli("synthetic-zlib-not-real-br-v1", synthetic_compress)


def fragments(snapshot: Snapshot) -> Iterator[tuple[str, str, dict[str, JsonValue]]]:
    for key, blob in snapshot.payloads.items():
        for table, entries in object_value(
            object_value(parse(blob.raw)).get("tables", {})
        ).items():
            for entry in array(entries):
                yield key, table, object_value(entry)


@pytest.fixture(scope="module")
def expanded(logical: tuple[Projection, Ownership]) -> tuple[Projection, Ownership]:
    projection, ownership = cloned(logical)
    projection.tables["image_variant"] = [
        {
            "image_id": "image",
            "size_key": size,
            "format": "webp",
            "width": width,
            "height": height,
            "bytes": 100,
        }
        for size, width, height in [
            ("art_m", 160, 120),
            ("art_s", 80, 60),
            ("card_l", 320, 448),
            ("card_m", 160, 224),
            ("card_s", 80, 112),
        ]
    ]
    binding = projection.tables["printing_image"][0]
    binding.update(
        publication_state="approved",
        availability="available",
        card_version=1,
        art_version=1,
        variants=[
            {key: row[key] for key in ("size_key", "width", "height")}
            for row in projection.tables["image_variant"]
        ],
    )
    homes = dict(ownership.printing_home)
    seed = deepcopy(projection.tables["printing"][0])
    for n in range(40):
        row = deepcopy(seed)
        row.update(id=f"p:synthetic-{n:02}", int_id=n + 100, card_no=f"SYN-{n + 100}")
        projection.tables["printing"].append(row)
        homes[string(row["id"])] = "family"
    projection.tables["printing"].sort(key=lambda row: string(row["id"]))
    return projection, Ownership(homes)


@pytest.fixture(scope="module")
def sharded(expanded: tuple[Projection, Ownership]) -> Snapshot:
    return export_snapshot(*expanded, BATCH, brotli=CODEC, format_version=MEDIA)


@pytest.fixture(scope="module")
def golden() -> tuple[dict[str, JsonValue], dict[str, bytes], JsonValue]:
    manifest = object_value(parse((GOLDEN / "manifest.json").read_bytes()))
    blobs = {
        string(f["key"]): canonical(
            parse((GOLDEN / "payloads" / Path(string(f["path"])).name).read_bytes())
        )
        for raw in array(manifest["files"])
        for f in (object_value(raw),)
    }
    return manifest, blobs, parse((GOLDEN / "expected-logical.json").read_bytes())


def test_independent_minor_golden(
    golden: tuple[dict[str, JsonValue], dict[str, bytes], JsonValue],
) -> None:
    manifest, blobs, expected = golden
    assert (
        read_snapshot(manifest, blobs) == expected == fixture("expected-logical.json")
    )
    attachments = {
        string(f["key"]): blobs[string(f["key"])]
        for raw in array(manifest["files"])
        for f in (object_value(raw),)
        if f["role"] in {"images", "programs"}
    }
    assert (
        read_text_all(
            manifest,
            canonical(parse((GOLDEN / "text-all.json").read_bytes())),
            attachments,
        )
        == expected
    )


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("format_version", "1.2.0"),
        ("min_reader_version", "1.0.0"),
        ("partitioning", {"algorithm": "sha256-mod-v1", "bucket_count": 32}),
        ("required_capabilities", ["column-partition-v1", "fragment-container-v1"]),
    ],
)
def test_explicit_profile_rejects_unknown_or_drift(
    golden: tuple[dict[str, JsonValue], dict[str, bytes], JsonValue],
    field: str,
    value: JsonValue,
) -> None:
    manifest, blobs, _ = golden
    with pytest.raises((ValueError, ValidationError)):
        read_snapshot(manifest | {field: value}, blobs)


def test_same_profile_produces_equal_bytes(
    sharded: Snapshot, expanded: tuple[Projection, Ownership]
) -> None:
    again = export_snapshot(
        *expanded,
        replace(BATCH, data_version="preview-20261003T010203Z-0001"),
        brotli=CODEC,
        format_version=MEDIA,
    )
    sharded.assert_identical(again)
    sharded.verify(expanded[0])
    assert object_value(sharded.manifest["partitioning"])["bucket_count"] == 64
    assert sharded.manifest["required_capabilities"] == [
        "column-partition-v1",
        "digital-same-name-links-v1",
        "fragment-container-v1",
        "image-entity-buckets-v1",
        "image-id-url-v1",
        "rules-name-on-demand-v1",
    ]


def test_physical_bands_retain_distinct_fragments_and_multiple_bases(
    sharded: Snapshot,
) -> None:
    printing = [(k, f) for k, t, f in fragments(sharded) if t == "printing"]
    bootstrap = [(k, f) for k, f in printing if f["partition"] == "bootstrap"]
    detail = [(k, f) for k, f in printing if f["partition"] == "detail"]
    assert len({k for k, _ in bootstrap}) == 1
    assert len({k for k, _ in detail}) == 2
    assert len(bootstrap) > 8
    for _, f in detail:
        base = object_value(f["base"])
        matching = next(b for _, b in bootstrap if b["bucket"] == f["bucket"])
        assert [array(row)[0] for row in array(f["rows"])] == list(
            range(len(array(matching["rows"])))
        )
        assert base["bucket"] == f["bucket"]


@pytest.mark.parametrize(
    ("owner", "width"), [("BP01", 32), ("CP04", 32), ("other", 64), ("BP010", 64)]
)
def test_owner_exception_is_fixed(
    expanded: tuple[Projection, Ownership], owner: str, width: int
) -> None:
    projection, ownership = cloned(expanded)
    family = projection.tables["product_family"][0].copy() | {"id": owner}
    projection.tables["product_family"].append(family)
    projection.tables["product_family"].sort(key=lambda row: string(row["id"]))
    result = export_snapshot(
        projection,
        Ownership(dict.fromkeys(ownership.printing_home, owner)),
        BATCH,
        format_version=MEDIA,
    )
    keys = {
        k
        for k, t, f in fragments(result)
        if t == "printing" and f["partition"] == "bootstrap"
    }
    assert keys == {
        f"bootstrap/bootstrap/home_set/{owner}/band/{n}" for n in range(64 // width)
    }


def test_image_variants_share_entity_bucket(
    sharded: Snapshot, expanded: tuple[Projection, Ownership]
) -> None:
    records = [
        (k, t, f)
        for k, t, f in fragments(sharded)
        if t in {"image_asset", "image_variant"}
    ]
    assert len({k for k, _, _ in records}) == 1
    for _, table, f in records:
        for row in array(f["rows"]):
            values = array(row)
            assert f["bucket"] == bucket([values[0]], 64)
            if table == "image_variant":
                assert bucket(values[:3], 64) != f["bucket"]
    assert expanded[0].tables["image_variant"]


def test_full_pk_variant_mutation_is_rejected_by_independent_reader(
    expanded: tuple[Projection, Ownership], monkeypatch: pytest.MonkeyPatch
) -> None:
    original = Layout.group

    def full_key(
        self: Layout, table: str, row: dict[str, JsonValue], partition: str
    ) -> Group:
        group = original(self, table, row, partition)
        if table == "image_variant":
            return replace(
                group,
                bucket=bucket([row["image_id"], row["size_key"], row["format"]], 64),
            )
        return group

    monkeypatch.setattr(Layout, "group", full_key)
    with pytest.raises(
        ValueError, match=r"^Fragment entity bucket does not match fixed profile$"
    ):
        export_snapshot(*expanded, BATCH, format_version=MEDIA)


def test_rules_name_tables_are_wholly_on_demand(sharded: Snapshot) -> None:
    rows = [
        (t, f)
        for _, t, f in fragments(sharded)
        if t in {"rules_name", "face_rules_name"}
    ]
    assert {t for t, _ in rows} == {"rules_name", "face_rules_name"}
    assert all(f["partition"] == "detail" and f["base"] is None for _, f in rows)


def test_pending_display_and_dual_faces_survive_minor() -> None:
    view, manifest, _ = pending_view()
    result = export_snapshot(
        Projection(view, object_value(fixture("payloads/config.json")), manifest),
        Ownership({"p:a": "set:a", "p:b": "set:a"}),
        BATCH,
        format_version=MEDIA,
    )
    result.verify(Projection(view, {}, {}))
    assert not view["face"][0]["current"]


def test_capacity_counts_all_fragments_and_region_whole_files(
    sharded: Snapshot,
) -> None:
    report = measure(sharded, brotli=CODEC)
    initial = [
        blob
        for key, blob in sharded.payloads.items()
        if key.startswith("bootstrap/") or key == "config"
    ]
    startup = object_value(object_value(report["startup_by_region"])["jp"])
    assert startup["raw"] == len(canonical(sharded.manifest)) + sum(
        len(b.raw) for b in initial
    )
    assert "bootstrap_br_1_mib" not in object_value(report["gates"])
    assert "bootstrap_gzip_1_mib" not in object_value(report["gates"])
    for raw in array(report["shards"]):
        item = object_value(raw)
        local = [f for k, _, f in fragments(sharded) if k == item["key"]]
        assert item["buckets"] == sorted({integer(f["bucket"]) for f in local})
        assert item["partitions"] == sorted({string(f["partition"]) for f in local})
    image_blobs = [b for k, b in sharded.payloads.items() if k.startswith("images/")]
    assert object_value(report["images_metadata"])["raw"] == sum(
        len(b.raw) for b in image_blobs
    )


def test_image_increment_changes_only_one_entity_file(
    expanded: tuple[Projection, Ownership], sharded: Snapshot
) -> None:
    projection, ownership = cloned(expanded)
    projection.tables["image_variant"][0]["bytes"] = 321
    changed = export_snapshot(
        projection, ownership, BATCH, brotli=CODEC, format_version=MEDIA
    )
    cost = update(sharded, changed, brotli=CODEC)
    assert cost["changed_keys"] == [
        next(k for k, t, _ in fragments(sharded) if t == "image_variant")
    ]
    assert all(
        changed.payloads[k] == blob
        for k, blob in sharded.payloads.items()
        if not k.startswith("images/detail/global/")
    )


def test_default_schema_does_not_silently_negotiate_new_minor() -> None:
    with pytest.raises(ValidationError):
        validate("Programs", {"format_version": "3.0.0", "entries": []})
    with pytest.raises(ValueError, match=r"^Unsupported snapshot format profile$"):
        profile("1.1.1")


def image_mutation(
    golden: tuple[dict[str, JsonValue], dict[str, bytes], JsonValue], change: str
) -> tuple[dict[str, JsonValue], dict[str, bytes]]:
    manifest, original, _ = golden
    manifest = deepcopy(manifest)
    blobs = original.copy()
    file = next(
        object_value(f)
        for f in array(manifest["files"])
        if string(object_value(f)["key"]).startswith("images/detail/global/")
    )
    key = string(file["key"])
    value = object_value(parse(blobs.pop(key)))
    if change == "mixed_profile":
        value["format_version"] = "1.0.0"
    elif change == "wrong_key":
        file["key"] = key + "/extra"
    else:
        entries = object_value(value["tables"])
        if change == "duplicate":
            array(entries["image_asset"]).append(
                deepcopy(array(entries["image_asset"])[0])
            )
        else:
            for raw_entries in entries.values():
                for raw in array(raw_entries):
                    f = object_value(raw)
                    f["bucket"] = (
                        64
                        if change == "out_of_range"
                        else (integer(f["bucket"]) + 1) % 64
                    )
            if change != "out_of_range":
                first = object_value(array(entries["image_asset"])[0])
                file["key"] = key.rsplit("/", 1)[0] + "/" + str(first["bucket"])
    encoded = canonical(value)
    file.update(
        sha256=digest(encoded),
        bytes=len(encoded),
        path="snapshots/blobs/" + digest(encoded)[7:] + ".json",
    )
    file["row_counts"] = [
        {
            "table": table,
            **{name: f[name] for name in ("owner", "bucket", "partition")},
            "count": len(array(f["rows"])),
        }
        for table, entries in object_value(value["tables"]).items()
        for entry in array(entries)
        for f in (object_value(entry),)
    ]
    blobs[string(file["key"])] = encoded
    array(manifest["files"]).sort(key=lambda item: string(object_value(item)["key"]))
    return manifest, blobs


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("mixed_profile", "Payload format differs from manifest"),
        ("wrong_key", "File key does not match fixed band recipe"),
        ("duplicate", "Duplicate fragment identity"),
        ("wrong_bucket", "Fragment entity bucket does not match fixed profile"),
        ("out_of_range", "Fragment role or bucket does not match profile"),
    ],
)
def test_resealed_semantic_mutations_rejected(
    golden: tuple[dict[str, JsonValue], dict[str, bytes], JsonValue],
    change: str,
    message: str,
) -> None:
    manifest, blobs = image_mutation(golden, change)
    with pytest.raises(ValueError, match="^" + message + "$"):
        read_snapshot(manifest, blobs)


@pytest.mark.parametrize("availability", ["unfetched", "missing"])
def test_non_available_images_retain_metadata_without_variants(
    expanded: tuple[Projection, Ownership], availability: str
) -> None:
    projection, ownership = cloned(expanded)
    projection.tables["image_variant"] = []
    for binding in projection.tables["printing_image"]:
        binding.update(
            availability=availability,
            publication_state="pending",
            variants=[],
            card_version=None,
            art_version=None,
        )
    result = export_snapshot(projection, ownership, BATCH, format_version=MEDIA)
    assert len([f for _, t, f in fragments(result) if t == "image_asset"]) == 1
    assert not any(t == "image_variant" for _, t, _ in fragments(result))
    result.verify(projection)


@pytest.mark.parametrize(
    "change", ["insert_printing", "base_value", "qa", "name_translation"]
)
def test_incremental_replacements_are_bounded(
    expanded: tuple[Projection, Ownership], sharded: Snapshot, change: str
) -> None:
    projection, ownership = cloned(expanded)
    if change == "insert_printing":
        row = deepcopy(projection.tables["printing"][0])
        row.update(id="p:incremental", card_no="SYN-999", int_id=999)
        projection.tables["printing"].append(row)
        projection.tables["printing"].sort(key=lambda r: string(r["id"]))
        ownership = Ownership(
            dict(ownership.printing_home) | {"p:incremental": "family"}
        )
        prefix = "bootstrap/bootstrap/home_set/family/"
    elif change == "base_value":
        projection.tables["printing"][0]["rarity_raw"] = "Synthetic changed rarity"
        prefix = "bootstrap/bootstrap/home_set/family/"
    elif change == "qa":
        projection.tables["qa_version"][0]["published_on"] = "2026-10-01"
        prefix = "text/detail/global/"
    else:
        names = {
            object_value(item)["translation_id"]
            for row in projection.tables["face_revision"]
            for item in array(row["translations"])
            if object_value(item)["field"] == "name"
        }
        translated = next(
            row for row in projection.tables["translation"] if row["id"] in names
        )
        text = next(
            r
            for r in projection.tables["text_unit"]
            if r["id"] == translated["text_unit_id"]
        )
        old = text["id"]
        text["text"] = "Synthetic revised translation"
        text["id"] = (
            "t:"
            + string(text["lang"])
            + ":"
            + digest(string(text["text"]).encode())[7:23]
        )
        translated["text_unit_id"] = text["id"]
        projection.tables["text_unit"].sort(key=lambda r: string(r["id"]))
        assert old != text["id"]
        prefix = "bootstrap/bootstrap/global/"
    result = export_snapshot(
        projection, ownership, BATCH, brotli=CODEC, format_version=MEDIA
    )
    keys = list(
        map(string, array(update(sharded, result, brotli=CODEC)["changed_keys"]))
    )
    assert any(k.startswith(prefix) for k in keys)
    assert not any(k.startswith("images/") for k in keys)
    assert len(keys) <= 4
    if change in {"insert_printing", "base_value"}:
        assert (
            len([k for k in keys if k.startswith("text/detail/home_set/family/")]) == 2
        )
        bases = [
            object_value(object_value(f["base"])["file"])
            for k, t, f in fragments(result)
            if t == "printing" and f["partition"] == "detail"
        ]
        assert len({canonical(b) for b in bases}) == 1
        assert keys == [
            "bootstrap/bootstrap/home_set/family/band/0",
            "text/detail/home_set/family/band/0",
            "text/detail/home_set/family/band/1",
        ]


def test_exact_fixed_band_width_matrix() -> None:
    selected = profile(MEDIA)
    assert list(
        starmap(
            selected.width,
            [
                ("images", "detail", "global", ""),
                ("images", "detail", "home_set", "BP01"),
                ("bootstrap", "bootstrap", "global", ""),
                ("bootstrap", "bootstrap", "home_set", "BP01"),
                ("bootstrap", "bootstrap", "home_set", "CP04"),
                ("bootstrap", "bootstrap", "home_set", "BP02"),
                ("text", "detail", "global", ""),
                ("text", "detail", "home_set", "BP02"),
                ("text", "history", "global", ""),
                ("text", "history", "home_set", "BP02"),
            ],
        )
    ) == [1, 32, 8, 32, 32, 64, 2, 32, 32, 32]


def test_shared_full_sha_bucket_goldens() -> None:
    for raw in array(parse((GOLDEN / "bucket-cases.json").read_bytes())):
        case = object_value(raw)
        key = array(case["key"])
        assert canonical(key).decode() == case["canonical"]
        assert digest(canonical(key)) == "sha256:" + string(case["sha256"])
        assert bucket(key, integer(case["n"])) == case["bucket"]


def test_pending_to_current_rebuilds_only_owner_bootstrap_and_details() -> None:
    view, metadata, _ = pending_view()
    config = object_value(fixture("payloads/config.json"))
    ownership = Ownership({"p:a": "set:a", "p:b": "set:a"})
    initial = export_snapshot(
        Projection(view, config, metadata), ownership, BATCH, format_version=MEDIA
    )
    settled = deepcopy(view)
    settled["face"][0].update(
        current=[
            {"region": "jp", "revision_id": "r:a2", "basis": "latest_adopted_wording"}
        ],
        wording=[],
    )
    settled["card_engine_support"][0]["region_blocks"] = []
    result = export_snapshot(
        Projection(settled, config, metadata), ownership, BATCH, format_version=MEDIA
    )
    keys = list(map(string, array(update(initial, result)["changed_keys"])))
    assert keys
    assert all("/home_set/set%3Aa/" in key for key in keys)
    assert any(key.startswith("bootstrap/") for key in keys)
    assert any(key.startswith("text/detail/") for key in keys)
    assert not any(key.startswith("images/") for key in keys)


def test_reader_detects_missing_types_and_payload(
    sharded: Snapshot, expanded: tuple[Projection, Ownership]
) -> None:
    # Both mutations pass the compressed-byte-independent fragment parser.
    manifest = deepcopy(sharded.manifest)
    file = next(
        object_value(f)
        for f in array(manifest["files"])
        if object_value(f)["role"] == "bootstrap"
        and string(object_value(f)["key"]).startswith("bootstrap/bootstrap/home_set/")
    )
    payload = object_value(parse(sharded.payloads[string(file["key"])].raw))
    assert payload["types"]
    payload["types"] = {}
    from sve_carddb.snapshot.reader import _container  # ruff: ignore[import-outside-top-level] -- exercise type-closure rejection before byte resealing

    with pytest.raises(
        ValueError, match=r"^Missing, unused or altered nested descriptor$"
    ):
        _container(file, payload)
    broken = sharded.payloads.copy()
    del broken[string(file["key"])]
    with pytest.raises(ValueError, match=r"^Payload set does not match manifest$"):
        replace(sharded, payloads=broken).verify(expanded[0])


def test_startup_exceeds_two_mib_is_reported_without_one_mib_gate(
    sharded: Snapshot,
) -> None:
    enormous = replace(
        sharded,
        payloads=sharded.payloads
        | {"config": replace(sharded.payloads["config"], br=b"x" * (2 * 1024 * 1024))},
    )
    report = measure(enormous, brotli=CODEC)
    startup = object_value(object_value(report["startup_by_region"])["jp"])
    assert startup["stop_for_maintainer"] is True
    assert startup["target_br_1_mib"] is False
    assert "bootstrap_br_1_mib" not in object_value(report["gates"])


def replaced_ids(value: JsonValue, names: dict[str, str]) -> JsonValue:
    if isinstance(value, dict):
        return {k: replaced_ids(v, names) for k, v in value.items()}
    if isinstance(value, list):
        return [replaced_ids(v, names) for v in value]
    return names.get(value, value) if isinstance(value, str) else value


def test_new_card_does_not_move_existing_owner_or_image_files(
    expanded: tuple[Projection, Ownership], sharded: Snapshot
) -> None:
    projection, ownership = cloned(expanded)
    names = {
        "card": "new:card",
        "face": "new:face",
        "face-rev": "new:revision",
        "printing": "new:printing",
    }
    source_card = projection.tables["card"][0]
    source_face = projection.tables["face"][0]
    source_revision = projection.tables["face_revision"][0]
    names.update(
        {
            string(source_card["id"]): "new:card",
            string(source_face["id"]): "new:face",
            string(source_revision["id"]): "new:revision",
            string(projection.tables["printing"][0]["id"]): "new:printing",
        }
    )
    for table in ("card", "face", "face_revision", "printing", "card_engine_support"):
        source = projection.tables[table][0]
        row = object_value(replaced_ids(source, names))
        if table == "printing":
            row["int_id"] = 1001
            row["card_no"] = "SYN-1001"
        projection.tables[table].append(row)
        projection.tables[table].sort(
            key=lambda r: string(r["id"] if "id" in r else r["card_id"])
        )
    result = export_snapshot(
        projection,
        Ownership(dict(ownership.printing_home) | {"new:printing": "family"}),
        BATCH,
        brotli=CODEC,
        format_version=MEDIA,
    )
    keys = array(update(sharded, result, brotli=CODEC)["changed_keys"])
    assert keys
    assert len(keys) <= 3
    assert all("/home_set/family/" in string(k) for k in keys)
