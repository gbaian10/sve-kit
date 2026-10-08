"""Exporter acceptance uses an independent reader and synthetic logical data."""

import gzip
from copy import deepcopy
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

if TYPE_CHECKING:
    from sve_carddb.snapshot.project import Projection

from pydantic import JsonValue

from sve_carddb.build_db import create_database
from sve_carddb.core.json import array, canonical, object_value, parse, string
from sve_carddb.snapshot.buckets import bucket
from sve_carddb.snapshot.export import (
    Batch,
    Ownership,
    Snapshot,
    _Files,
    _ordered,
    export_snapshot,
)
from sve_carddb.snapshot.export.compression import compress
from sve_carddb.snapshot.export.layout import Layout
from sve_carddb.snapshot.export.measure import measure, update
from sve_carddb.snapshot.media import prepare_media
from sve_carddb.snapshot.reader import read_text_all

from .snapshot_project_fixtures import populate, schema
from .test_snapshot_project import projected

BATCH = Batch("preview-20261002T010203Z-0001", "2026-10-02T01:02:03Z", ("jp",))


@pytest.fixture(scope="module")
def logical() -> tuple[Projection, Ownership]:
    with create_database(schema()) as db:
        with db.transaction():
            populate(db)
        projection = projected(db)
        projection = replace(
            projection,
            tables=projection.tables
            | {
                "printing_image": [
                    row | {"availability": "unfetched", "publication_state": "pending"}
                    for row in projection.tables["printing_image"]
                ],
                "image_variant": [],
            },
        )
        prepared = prepare_media(projection, None, revision=1).projection
        return prepared, Ownership.from_database(db, projection)


@pytest.fixture(scope="module")
def exported(logical: tuple[Projection, Ownership]) -> Snapshot:
    projection, ownership = logical
    return export_snapshot(projection, ownership, BATCH)


def test_join_equals_public_projection(
    exported: Snapshot, logical: tuple[Projection, Ownership]
) -> None:
    exported.verify(logical[0])


def test_union_reader_equals_individual_shards(
    exported: Snapshot, logical: tuple[Projection, Ownership]
) -> None:
    attachments = {
        key: blob.raw
        for key, blob in exported.payloads.items()
        if key == "programs" or key.startswith("images/")
    }
    assert (
        read_text_all(exported.manifest, exported.text_all.raw, attachments)
        == logical[0].tables
    )


def test_clean_exports_are_exact_bytes(
    exported: Snapshot, logical: tuple[Projection, Ownership]
) -> None:
    projection, ownership = logical
    again = export_snapshot(
        projection,
        ownership,
        replace(
            BATCH,
            data_version="preview-20261002T010204Z-0001",
            published_at="2026-10-02T01:02:04Z",
        ),
    )
    exported.assert_identical(again)
    assert exported.manifest != again.manifest


def cloned(logical: tuple[Projection, Ownership]) -> tuple[Projection, Ownership]:

    projection, ownership = logical
    rows: dict[str, JsonValue] = {
        name: list(rows) for name, rows in projection.tables.items()
    }
    rows = object_value(parse(canonical(rows)))
    return replace(
        projection,
        tables={
            name: [object_value(row) for row in array(values)]
            for name, values in rows.items()
        },
    ), ownership


@pytest.mark.parametrize("encoding", ["raw", "br", "gzip"])
def test_byte_comparison_catches_one_encoding_change(
    exported: Snapshot, encoding: str
) -> None:
    blobs = exported.payloads.copy()
    first = next(iter(blobs))
    original = getattr(blobs[first], encoding)
    blobs[first] = replace(blobs[first], **{encoding: (original or b"") + b"changed"})
    with pytest.raises(ValueError, match="Clean exports differ"):
        exported.assert_identical(replace(exported, payloads=blobs))


def test_union_bytes_are_also_compared(exported: Snapshot) -> None:
    with pytest.raises(ValueError, match="Clean exports differ"):
        exported.assert_identical(
            replace(
                exported,
                text_all=replace(
                    exported.text_all, raw=exported.text_all.raw + b"changed"
                ),
            )
        )


def test_independent_expected_join_detects_lost_field_value(
    exported: Snapshot, logical: tuple[Projection, Ownership]
) -> None:
    projection, _ = cloned(logical)
    projection.tables["printing"][0]["rarity_raw"] = "different synthetic rarity"
    with pytest.raises(ValueError, match="join differs"):
        exported.verify(projection)


def test_cross_shard_cycles_rejected(
    exported: Snapshot, logical: tuple[Projection, Ownership]
) -> None:

    manifest = object_value(parse(canonical(exported.manifest)))
    files = {object_value(f)["key"]: object_value(f) for f in array(manifest["files"])}
    bootstrap = next(key for key in files if string(key).startswith("bootstrap/"))
    for key, target in (("config", bootstrap), (bootstrap, "config")):
        files[key]["dependencies"] = [
            {"key": target, "sha256": files[target]["sha256"]}
        ]
    with pytest.raises(ValueError, match="Cyclic dependencies"):
        replace(exported, manifest=manifest).verify(logical[0])


def test_unknown_dependency_cannot_use_another_snapshot(
    exported: Snapshot, logical: tuple[Projection, Ownership]
) -> None:

    manifest = object_value(parse(canonical(exported.manifest)))
    object_value(array(manifest["files"])[0])["dependencies"] = [
        {"key": "missing", "sha256": "sha256:" + "0" * 64}
    ]
    with pytest.raises(KeyError, match="missing"):
        replace(exported, manifest=manifest).verify(logical[0])


@pytest.mark.parametrize(
    "change", ["missing", "extra", "duplicate", "bad_face_ordinal", "bad_owner"]
)
def test_producer_does_not_repair_invalid_public_view(
    logical: tuple[Projection, Ownership], change: str
) -> None:

    projection, ownership = cloned(logical)
    if change == "missing":
        del projection.tables["printing"][0]["premium"]
    elif change == "extra":
        projection.tables["printing"][0]["internal_hash"] = "secret"
    elif change == "duplicate":
        projection.tables["printing"].append(projection.tables["printing"][0].copy())
    elif change == "bad_face_ordinal":
        physical = object_value(array(projection.tables["printing"][0]["faces"])[0])
        physical["face_id"] = "missing:face"
    else:
        ownership = Ownership({})
    with pytest.raises((ValueError, KeyError)):
        export_snapshot(projection, ownership, BATCH)


def test_candidate_cannot_mint_a_formal_release(
    logical: tuple[Projection, Ownership],
) -> None:
    with pytest.raises(ValueError, match="preview"):
        export_snapshot(*logical, replace(BATCH, data_version="20261002T010203Z-0001"))


def rename_printing(value: JsonValue, old: str, new: str) -> None:
    if isinstance(value, dict):
        for field, item in value.items():
            if field in {"printing_id", "default_printing_id"} and item == old:
                value[field] = new
            else:
                rename_printing(item, old, new)
    elif isinstance(value, list):
        for item in value:
            rename_printing(item, old, new)


def test_order_changes_rebind_every_positional_detail(
    logical: tuple[Projection, Ownership], exported: Snapshot
) -> None:

    projection, ownership = cloned(logical)
    old = string(projection.tables["printing"][0]["id"])
    new = "z:synthetic-renamed"
    projection.tables["printing"][0]["id"] = new

    for rows in projection.tables.values():
        for row in rows:
            rename_printing(row, old, new)
    homes = dict(ownership.printing_home)
    homes[new] = homes.pop(old)
    projection.tables["printing"].sort(key=lambda row: string(row["id"]))
    changed = export_snapshot(projection, Ownership(homes), BATCH)
    updates = update(exported, changed)
    assert any(
        string(key).startswith("text/detail/home_set/")
        for key in array(updates["changed_keys"])
    )
    keys = [
        string(key)
        for key in array(updates["changed_keys"])
        if string(key).startswith("text/detail/home_set/")
    ]
    for key in keys:
        assert changed.payloads[key].raw != exported.payloads[key].raw


def test_inserting_a_sorted_base_row_shifts_old_indices(
    logical: tuple[Projection, Ownership], exported: Snapshot
) -> None:
    projection, ownership = cloned(logical)
    inserted = deepcopy(projection.tables["printing"][0])
    identifier = next(
        f"a:synthetic-{n}"
        for n in range(1000)
        if bucket([f"a:synthetic-{n}"], 64)
        == bucket([projection.tables["printing"][0]["id"]], 64)
    )
    inserted.update(id=identifier, int_id=999, card_no="TEST-0")
    projection.tables["printing"].insert(0, inserted)
    changed = export_snapshot(
        projection,
        Ownership(dict(ownership.printing_home) | {identifier: "family"}),
        BATCH,
    )
    key = next(
        key
        for key in changed.payloads
        if key.startswith("text/detail/home_set/")
        and "printing"
        in object_value(object_value(parse(changed.payloads[key].raw))["tables"])
    )
    details = object_value(
        array(
            object_value(object_value(parse(changed.payloads[key].raw))["tables"])[
                "printing"
            ]
        )[0]
    )
    assert [array(row)[0] for row in array(details["rows"])] == [0, 1]
    assert changed.payloads[key].raw != exported.payloads[key].raw


def test_capacity_counts_metadata_and_never_double_counts_text_all(
    exported: Snapshot,
) -> None:
    report = measure(exported)
    assert object_value(report["text_total"])["raw"] == sum(
        len(blob.raw)
        for key, blob in exported.payloads.items()
        if key != "programs" and not key.startswith("images/")
    ) + len(canonical(exported.manifest))
    assert object_value(report["gates"])["br_8_mib"] is False
    assert object_value(report["gates"])["shards_512_kib"] is True


def test_printing_uses_registered_home_even_when_card_has_another_home(
    logical: tuple[Projection, Ownership],
) -> None:
    projection, _ = cloned(logical)
    family = projection.tables["product_family"][0].copy()
    family["id"] = "other_family"
    projection.tables["product_family"].append(family)
    result = export_snapshot(projection, Ownership({"printing": "other_family"}), BATCH)
    boot = [
        object_value(object_value(parse(blob.raw))["tables"])
        for key, blob in result.payloads.items()
        if key.startswith("bootstrap/")
    ]
    printing = next(
        object_value(array(value["printing"])[0])
        for value in boot
        if "printing" in value
    )
    card = next(
        object_value(array(value["card"])[0]) for value in boot if "card" in value
    )
    assert object_value(printing["owner"])["id"] == "other_family"
    assert object_value(card["owner"])["id"] == "family"


def test_pending_display_name_and_facets_are_bootstrapped_without_current() -> None:
    from sve_carddb.snapshot.project import Projection  # ruff: ignore[import-outside-top-level] -- construct a public view independently of the producer

    from .snapshot_contract_fixtures import fixture  # ruff: ignore[import-outside-top-level] -- shared golden contains only synthetic text
    from .test_snapshot_wording import pending_view  # ruff: ignore[import-outside-top-level] -- independent accepted public pending case

    view, manifest, _ = pending_view()
    config = object_value(fixture("payloads/config.json"))
    result = export_snapshot(
        Projection(view, config, manifest),
        Ownership({"p:a": "set:a", "p:b": "set:a"}),
        BATCH,
    )
    boot = [
        object_value(object_value(parse(blob.raw))["tables"])
        for key, blob in result.payloads.items()
        if key.startswith("bootstrap/")
    ]
    revision_rows = [
        array(row)
        for value in boot
        if "face_revision" in value
        for fragment in array(value["face_revision"])
        for row in array(object_value(fragment)["rows"])
    ]
    assert {row[0] for row in revision_rows} == {"r:a2", "r:b1"}
    # Names of non-display candidates remain on demand; only exact display names are mandatory.
    text_rows = [
        array(row)
        for value in boot
        if "text_unit" in value
        for fragment in array(value["text_unit"])
        for row in array(object_value(fragment)["rows"])
    ]
    name_ids = {row[3] for row in revision_rows}
    assert name_ids <= {row[0] for row in text_rows}
    assert any("vocabulary" in value for value in boot)
    history = [
        object_value(object_value(parse(blob.raw))["tables"])
        for key, blob in result.payloads.items()
        if key.startswith("text/history/")
    ]
    assert {
        array(row)[0]
        for value in history
        for fragment in array(value["face_revision"])
        for row in array(object_value(fragment)["rows"])
    } == {"r:a1"}


def test_gzip_roundtrip_and_header_are_deterministic(exported: Snapshot) -> None:
    for blob in [*exported.payloads.values(), exported.text_all]:
        assert gzip.decompress(blob.gzip) == blob.raw
        assert blob.gzip[3] == 0
        assert blob.gzip[4:8] == b"\0\0\0\0"


def test_gzip_timestamp_is_fixed_across_seconds(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    real_compress = gzip.compress
    seconds = (1_735_689_600, 1_735_689_602)
    clock = iter(seconds)
    observed: list[int] = []

    # Python defaults vary; require explicit mtime even when omitted means wall-clock time.
    def wall_clock_compress(
        data: bytes, compresslevel: int = 9, *, mtime: int | None = None
    ) -> bytes:
        now = next(clock)
        observed.append(now)
        return real_compress(
            data, compresslevel=compresslevel, mtime=now if mtime is None else mtime
        )

    raw = b"synthetic cross-second gzip payload"
    assert real_compress(raw, mtime=seconds[0]) != real_compress(raw, mtime=seconds[1])
    monkeypatch.setattr(gzip, "compress", wall_clock_compress)
    first = compress(raw, None)
    second = compress(raw, None)
    assert observed == list(seconds)
    assert first.gzip == second.gzip
    assert first.gzip[4:8] == second.gzip[4:8] == b"\0\0\0\0"


def test_gzip_bytes_use_level_nine() -> None:
    raw = b"synthetic repeated gzip payload " * 64
    expected = gzip.compress(raw, compresslevel=9, mtime=0)
    assert expected != gzip.compress(raw, compresslevel=6, mtime=0)
    assert compress(raw, None).gzip == expected


@pytest.mark.parametrize("table", ["printing", "printing_product"])
def test_primary_key_boundary_rejects_duplicates(
    logical: tuple[Projection, Ownership], table: str
) -> None:
    rows = logical[0].tables[table]
    assert rows
    with pytest.raises(ValueError, match="Duplicate public primary key"):
        _ordered(table, [*rows, rows[0].copy()])


def test_file_key_collision_preserves_the_first_payload() -> None:
    files = _Files(None)
    first = files.add("config", "config", {"synthetic": "first"}, [])
    original = files.payloads["config"]
    with pytest.raises(ValueError, match="Duplicate logical file key"):
        files.add("config", "config", {"synthetic": "replacement"}, [])
    assert files.files == {"config": first}
    assert files.payloads == {"config": original}


def test_ownership_boundary_rejects_one_missing_printing(
    logical: tuple[Projection, Ownership],
) -> None:
    projection, ownership = cloned(logical)
    extra = projection.tables["printing"][0].copy()
    extra["id"] = "p:synthetic-without-owner"
    projection.tables["printing"].append(extra)
    assert ownership.printing_home
    with pytest.raises(ValueError, match="Exact public printing ownership required"):
        Layout(projection.tables, ownership, 1)


def test_explicit_compressor_recipe_and_bytes_are_used(
    logical: tuple[Projection, Ownership],
) -> None:
    import zlib  # ruff: ignore[import-outside-top-level] -- a synthetic codec tests injection without adding a Brotli dependency

    from sve_carddb.snapshot.export import Brotli  # ruff: ignore[import-outside-top-level] -- real Brotli q11 is exercised by the private capacity run

    codec = Brotli("synthetic-test-codec-v1", zlib.compress)
    first = export_snapshot(*logical, BATCH, brotli=codec)
    second = export_snapshot(*logical, BATCH, brotli=codec)
    first.assert_identical(second)
    assert all(
        blob.br is not None and zlib.decompress(blob.br) == blob.raw
        for blob in first.payloads.values()
    )
    assert first.compression_recipe["brotli"] == codec.version
    assert object_value(measure(first, brotli=codec)["text_total"])["br"] is not None
    with pytest.raises(ValueError, match="same compressor recipe"):
        measure(first)


def test_foreign_printing_home_is_rejected(
    logical: tuple[Projection, Ownership],
) -> None:
    with pytest.raises(ValueError, match="public product family"):
        export_snapshot(logical[0], Ownership({"printing": "unknown_family"}), BATCH)


def test_name_translation_and_facet_dictionary_closure(
    logical: tuple[Projection, Ownership], exported: Snapshot
) -> None:
    from sve_carddb.snapshot.contract import decode, row_type  # ruff: ignore[import-outside-top-level] -- decode fixed schema independently of exporter selection

    records: dict[str, list[dict[str, JsonValue]]] = {}
    for key, blob in exported.payloads.items():
        if not key.startswith("bootstrap/"):
            continue
        for table, fragments in object_value(
            object_value(parse(blob.raw))["tables"]
        ).items():
            for raw in array(fragments):
                fragment = object_value(raw)
                records.setdefault(table, []).extend(
                    decode(row_type(table, "bootstrap"), row)
                    for row in array(fragment["rows"])
                )
    name_translations = {
        object_value(item)["translation_id"]
        for revision in records["face_revision"]
        for item in array(revision["translations"])
        if object_value(item)["translation_id"] is not None
    }
    assert name_translations
    translations = {row["id"]: row for row in records["translation"]}
    texts = {row["id"] for row in records["text_unit"]}
    for identifier in name_translations:
        selected = translations[identifier]
        assert selected["source_unit_id"] in texts
        assert selected["text_unit_id"] in texts
    assert sorted(records["vocabulary"], key=canonical) == sorted(
        logical[0].tables["vocabulary"], key=canonical
    )
    assert records["keyword"] == logical[0].tables["keyword"]


def test_clean_comparison_rejects_recipe_change_even_with_same_bytes(
    exported: Snapshot,
) -> None:
    with pytest.raises(ValueError, match="Clean exports differ"):
        exported.assert_identical(
            replace(
                exported,
                compression_recipe=exported.compression_recipe
                | {"gzip": "another-version"},
            )
        )


@pytest.mark.parametrize(
    "field", ["default_printing_id", "effect_unit_id", "type_code"]
)
def test_export_validates_semantics_beyond_tuple_shape(
    logical: tuple[Projection, Ownership], field: str
) -> None:
    projection, ownership = cloned(logical)
    if field == "default_printing_id":
        object_value(array(projection.tables["card"][0]["regions"])[0])[field] = (
            "missing:printing"
        )
    else:
        projection.tables["face_revision"][0][field] = (
            "missing_synthetic"
            if field == "type_code"
            else "missing:synthetic-reference"
        )
    with pytest.raises(ValueError, match=r"Dangling|Vocabulary"):
        export_snapshot(projection, ownership, BATCH)


def test_manifest_cannot_claim_a_different_public_region(
    logical: tuple[Projection, Ownership],
) -> None:
    with pytest.raises(ValueError, match="manifest scope"):
        export_snapshot(*logical, replace(BATCH, regions=("en",)))


def test_extra_nested_audit_field_is_not_silently_removed() -> None:
    from sve_carddb.snapshot.export.wire import encode  # ruff: ignore[import-outside-top-level] -- test the independent fixed tuple boundary with a hand-written record

    with pytest.raises(ValueError, match="whitelist"):
        encode(
            "Correction",
            {
                "field": "name",
                "corrected_from": "synthetic",
                "is_corrected": True,
                "reason": "synthetic",
                "source_url": None,
                "private_audit": "must not ship",
            },
        )


@pytest.mark.parametrize("format_version", ["2.0.0"])
def test_single_oversized_fragment_is_reported_instead_of_silently_dropped(
    format_version: str,
    logical: tuple[Projection, Ownership],
) -> None:
    from sve_carddb.core.json import digest  # ruff: ignore[import-outside-top-level] -- retain the exact text-ID contract after the mutation
    from sve_carddb.snapshot.export.measure import SHARD_LIMIT  # ruff: ignore[import-outside-top-level] -- synthetic data exercises the real shard gate

    projection, ownership = cloned(logical)
    physical = object_value(array(projection.tables["printing"][0]["faces"])[0])
    text = "synthetic " * (SHARD_LIMIT // 10 + 1)
    identifier = "t:ja:" + digest(text.encode())[7:23]
    projection.tables["text_unit"].append(
        {"id": identifier, "lang": "ja", "text": text}
    )
    projection.tables["text_unit"].sort(key=lambda row: string(row["id"]))
    physical["flavor_unit_id"] = identifier
    result = export_snapshot(
        projection, ownership, BATCH, format_version=format_version
    )
    report = measure(result)
    assert object_value(report["gates"])["shards_512_kib"] is False
    assert array(report["oversized_shards"])


def test_extra_public_collection_is_rejected(
    logical: tuple[Projection, Ownership],
) -> None:
    projection, ownership = cloned(logical)
    projection.tables["private_audit"] = []
    with pytest.raises(ValueError, match="collection whitelist"):
        export_snapshot(projection, ownership, BATCH)
