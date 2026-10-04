"""Data and evidence counterexamples with correctly re-signed synthetic envelopes."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.registry.build import build
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.registry.storage import (
    Index,
    Shard,
    load,
    plan_files,
    read_yaml,
    write_files,
)

from .registry_snapshot_fixtures import edit_record, rewrite
from .registry_snapshot_fixtures import registry_root as registry_root  # ruff: ignore[useless-import-alias] -- expose synthetic pytest fixture
from .test_registry import card
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- expose dependency of the synthetic registry fixture

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.registry.review import Inputs
    from sve_carddb.registry.storage import Entry


@pytest.mark.parametrize(
    ("kind", "field", "value"),
    [
        ("card", "layout", "triple"),
        ("card", "identity_state", "fresh"),
        ("card", "home_set_id", ""),
        ("card", "id", "p:" + "0" * 32),
        ("face", "ordinal", True),
        ("face", "ordinal", -1),
        ("face", "side", "other"),
        ("printing", "card_no", ""),
        ("printing", "variant_key", 1),
        ("card_int_id", "int_id", True),
        ("card_int_id", "int_id", 4294967296),
        ("card_int_id", "allocated_at", "2026-10-01"),
        ("region_mapping_review", "coverage_hash", "sha256:wrong"),
        ("region_mapping_review", "as_of", "yesterday"),
        ("region_mapping_review", "coverage_hash", "sha256:" + "0" * 64 + "\n"),
        ("art", "classification", "unknown"),
        ("card_related", "relation", "mentions"),
        ("card_related", "source_kind", "official"),
        ("card_related", "suggested_count", 1),
        ("card_related", "target_printing_id", "p:" + "0" * 32),
        ("card_related", "dsl_id", "document"),
        ("source_correction", "field", "cost"),
        ("source_correction", "corrected_value", 1),
        ("source_correction", "source_hash_recipe", "html-sha256"),
        ("source_correction", "evidence", []),
    ],
)
def test_strict_record_domains(
    registry_root: Path, kind: str, field: str, value: JsonValue
) -> None:
    def damage(entry: Entry) -> None:
        entry.data[field] = value

    edit_record(registry_root, kind, damage)
    with pytest.raises(ValueError, match="Invalid registry data"):
        load_registry(registry_root)


@pytest.mark.parametrize("value", [1, 20000, 20001, 60000, 100000, 4294967295])
def test_en_allocation_uses_registered_region_range(
    registry_root: Path, value: int
) -> None:
    def damage(entry: Entry) -> None:
        entry.data["int_id"] = value

    # The first owner is BP02, whose first allocation is EN in the synthetic input.
    _, entries = load(registry_root)
    chosen = next(e for e in entries.values() if e.kind == "card_int_id")
    printing = entries["printing:" + str(chosen.data["printing_id"])]
    assert printing.data["region"] == "en"
    edit_record(registry_root, "card_int_id", damage)
    with pytest.raises(ValueError, match="out-of-range"):
        load_registry(registry_root)


@pytest.mark.parametrize("field", ["card_no", "region", "recipe", "observation_hash"])
def test_observation_identity_and_shape(
    inputs: Inputs, tmp_path: Path, field: str
) -> None:
    inputs.en.clear()
    inputs.mapping.targets.clear()
    inputs.mapping.original_art.clear()
    inputs.mapping.reskins.clear()
    write_files(plan_files(tmp_path, build(inputs, {})))

    def damage(entry: Entry) -> None:
        observed = entry.data["observation"]
        assert isinstance(observed, dict)
        observed[field] = {
            "card_no": "different",
            "region": "en",
            "recipe": "other",
            "observation_hash": "bad",
        }[field]

    edit_record(tmp_path, "printing", damage, region="jp")
    with pytest.raises(ValueError, match=r"Printing observation|Invalid registry data"):
        load_registry(tmp_path)


@pytest.mark.parametrize(
    "field", ["observation_hash", "rules_hash", "region", "card_no"]
)
def test_en_target_requires_exact_registry_observation(
    registry_root: Path, field: str
) -> None:
    def damage(entry: Entry) -> None:
        review = entry.data["cross_region_review"]
        assert isinstance(review, dict)
        target = review["target_observation"]
        assert isinstance(target, dict)
        target[field] = {
            "observation_hash": "sha256:" + "9" * 64,
            "rules_hash": "sha256:" + "9" * 64,
            "region": "en",
            "card_no": "BP02-070",
        }[field]

    edit_record(registry_root, "printing", damage, region="en")
    with pytest.raises(ValueError, match="target observation"):
        load_registry(registry_root)


def test_en_target_cannot_be_guessed_by_stripping_suffix(registry_root: Path) -> None:
    def damage(entry: Entry) -> None:
        review = entry.data["cross_region_review"]
        assert isinstance(review, dict)
        review["target_jp_card_no"] = "BP02-070"

    edit_record(registry_root, "printing", damage, region="en")
    with pytest.raises(ValueError, match="target disagrees"):
        load_registry(registry_root)


@pytest.mark.parametrize("damage", ["wrong_observation", "duplicate_use"])
def test_art_evidence_is_bound_to_registered_use(
    registry_root: Path, damage: str
) -> None:
    def change(entry: Entry) -> None:
        if damage == "wrong_observation":
            observed = entry.data["observation"]
            assert isinstance(observed, dict)
            observed["observation_hash"] = "sha256:" + "9" * 64
        else:
            uses = entry.data["uses"]
            assert isinstance(uses, list)
            uses.append(uses[0])

    edit_record(registry_root, "art", change)
    with pytest.raises(ValueError, match="Art observation"):
        load_registry(registry_root)


@pytest.mark.parametrize("damage", ["owner", "ordinal", "side"])
def test_face_identity_constraints(registry_root: Path, damage: str) -> None:
    def change(entry: Entry) -> None:
        values: dict[str, JsonValue] = {
            "owner": "c:" + "9" * 32,
            "ordinal": 1,
            "side": "back",
        }
        entry.data[
            {"owner": "card_id", "ordinal": "ordinal", "side": "side"}[damage]
        ] = values[damage]

    edit_record(registry_root, "face", change)
    with pytest.raises(ValueError, match=r"Orphan face|cardinality|side mismatch"):
        load_registry(registry_root)


def test_reskin_reverse_requires_independent_rejection(registry_root: Path) -> None:

    index = Index.model_validate(read_yaml(registry_root / "ids/index.yaml"))
    name = next(name for name in index.includes if "/card_related/" in name)
    path = registry_root / name
    shard = Shard.model_validate(read_yaml(path))
    reverse = shard.records[0].model_copy(deep=True)
    reverse.data["id"] = "r:" + "9" * 32
    reverse.record_key = "card_related:" + str(reverse.data["id"])
    reverse.data["from_card_id"], reverse.data["to_card_id"] = (
        reverse.data["to_card_id"],
        reverse.data["from_card_id"],
    )
    evidence = reverse.data["evidence"]
    assert isinstance(evidence, list)
    for item in evidence:
        assert isinstance(item, dict)
        item["role"] = "to" if item["role"] == "from" else "from"
    shard.records.append(reverse)
    rewrite(registry_root, path, shard, resign=True)
    with pytest.raises(ValueError, match="Reverse reskin"):
        load_registry(registry_root)


def test_source_face_map_is_not_replaced_with_ordinal(
    inputs: Inputs, tmp_path: Path
) -> None:

    double = card("DF01-001", "Front")
    double.faces.append(card("unused", "Back").faces[0])
    inputs.jp[double.number] = double
    write_files(plan_files(tmp_path, build(inputs, {})))
    index, entries = load(tmp_path)
    printing = next(
        e
        for e in entries.values()
        if e.kind == "printing" and e.data["card_no"] == "DF01-001"
    )
    maps = printing.data["source_face_map"]
    assert isinstance(maps, list)
    assert isinstance(maps[0], dict)
    assert isinstance(maps[1], dict)
    face_ids = maps[1]["face_id"], maps[0]["face_id"]

    def swap(entry: Entry) -> None:
        entry.data["source_face_map"] = [
            {"source_index": i, "face_id": face_id}
            for i, face_id in enumerate(face_ids)
        ]

    # DF01 is the only printing owner with this card number.
    path = tmp_path / "registry/printing/DF01/001.yaml"

    shard = Shard.model_validate(read_yaml(path))
    swap(shard.records[0])
    rewrite(tmp_path, path, shard, resign=True)
    snapshot = load_registry(tmp_path)
    assert (
        snapshot.records[printing.record_key].entry().data["source_face_map"]
        == shard.records[0].data["source_face_map"]
    )
    assert snapshot.files.index().next_int_id == index.next_int_id


def test_en_only_cannot_carry_a_target_observation(registry_root: Path) -> None:
    path = registry_root / "registry/printing/GF01/001.yaml"
    shard = Shard.model_validate(read_yaml(path))
    review = shard.records[0].data["cross_region_review"]
    assert isinstance(review, dict)
    assert review["target_jp_card_no"] is None
    review["target_observation"] = shard.records[0].data["observation"]
    rewrite(registry_root, path, shard, resign=True)
    with pytest.raises(ValueError, match="English-only review"):
        load_registry(registry_root)
