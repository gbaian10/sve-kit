"""Offline identity, immutable allocation, receipt and YAML regression tests."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from ruamel.yaml.error import YAMLError

from sve_carddb.registry import allocation, storage
from sve_carddb.registry.allocation import IdRange, region_allocations
from sve_carddb.registry.build import allocation_order, build, permanent_id, string
from sve_carddb.registry.corrections import project_corrections
from sve_carddb.registry.inputs import (
    Card,
    Face,
    Mapping,
    read_cards,
    read_mapping,
    validate_mapping,
)
from sve_carddb.registry.review import (
    Correction,
    Inputs,
    Receipt,
    file_hash,
    read_inputs,
    validation_cards,
)
from sve_carddb.registry.storage import (
    TARGET_BYTES,
    Entry,
    Index,
    Shard,
    encode,
    load,
    plan_files,
    read_yaml,
    relayout,
    write_files,
    yaml_parser,
)
from sve_carddb.registry.validate import check_cursors, validate

from .official_registry_fixtures import detached_entries

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.registry.snapshot import RegistrySnapshot


def card(number: str, name: str, *, english: bool = False, text: str = "Rule.") -> Card:
    face = Face(
        name=name,
        image="/images/BP01/example.png",
        text=text,
        card_class="ウィッチ",
        card_type="フォロワー",
        cost="2",
        power="2",
        hp="1",
    )
    if english:
        face.info = {"Class": "Runecraft", "Card Type": "Follower", "Trait": "Mage"}
        face.stats = {"cost": "2", "power": "2", "hp": "1"}
    return Card(number=number, faces=[face])


def int_ids(entries: list[Entry]) -> dict[str, int]:
    printings = {
        string(entry.data, "id"): string(entry.data, "region")
        + ":"
        + string(entry.data, "card_no")
        for entry in entries
        if entry.kind == "printing"
    }
    return {
        printings[string(entry.data, "printing_id")]: int(str(entry.data["int_id"]))
        for entry in entries
        if entry.kind == "card_int_id"
    }


@pytest.fixture
def inputs() -> Inputs:
    return Inputs(
        jp={
            "BP02-071": card("BP02-071", "名前"),
            "PR-001": card("PR-001", "名前", text="Rule. (Reminder.)"),
        },
        en={
            "BP02-070EN": card("BP02-070EN", "Name", english=True),
            "GF01-001EN": card("GF01-001EN", "Reskin", english=True),
        },
        mapping=Mapping(
            targets={"BP02-070EN": "BP02-071", "GF01-001EN": None},
            original_art={"BP02-070EN"},
            reskins={"GF01-001EN": "BP02-071"},
        ),
        receipt=Receipt(
            policy="identity-init-2026-09-28-v1",
            reviewed_by="test-reviewer",
            reviewed_on="2026-09-28",
            input_hashes={"jp": "sha256:" + "0" * 64},
        ),
    )


def test_rerun_and_append_preserve_all_old_ids(inputs: Inputs, tmp_path: Path) -> None:
    inputs.mapping.reskins = {}
    first = build(inputs, {})
    validate(first)
    files = plan_files(tmp_path, first, "reviewer", "2026-09-28")
    write_files(files)
    index, old = load(tmp_path)
    assert index.next_int_id == {"en": 60003, "jp": 20003}
    assert plan_files(tmp_path, build(inputs, old), "reviewer", "2026-09-28") == {}
    before = {path: path.read_bytes() for path in files}
    inputs.jp["AA01-001"] = card("AA01-001", "名前")
    inputs.jp["AA01-002"] = card("AA01-002", "別名")
    inputs.jp["BP02-001"] = card("BP02-001", "別名")
    inputs.receipt.reviewed_on = "2026-09-29"
    inputs.receipt.input_hashes["jp"] = "sha256:" + "1" * 64
    second = build(inputs, old)
    validate(second)
    new = {entry.record_key: entry for entry in second}
    assert all(new[key] == entry for key, entry in old.items())
    added = new["printing:" + permanent_id("p", "jp:AA01-001")]
    original = old["printing:" + permanent_id("p", "jp:BP02-071")]
    assert added.data["card_id"] == original.data["card_id"]
    appended = plan_files(tmp_path, second, "reviewer", "2026-09-29")
    assert tmp_path / "ids" / "BP02" / "002.yaml" in appended
    write_files(appended)
    assert all(
        path.read_bytes() == value
        for path, value in before.items()
        if path.name != "index.yaml"
    )
    assert load(tmp_path)[0].next_int_id == {"en": 60003, "jp": 20006}
    # Earlier card numbers still append above the old high-water mark.
    assert int_ids(second) == {
        "jp:BP02-071": 20001,
        "jp:PR-001": 20002,
        "jp:AA01-001": 20003,
        "jp:AA01-002": 20004,
        "jp:BP02-001": 20005,
        "en:BP02-070EN": 60001,
        "en:GF01-001EN": 60002,
    }


def test_source_changes_and_deletions_fail_closed(
    inputs: Inputs, tmp_path: Path
) -> None:
    first = build(inputs, {})
    write_files(plan_files(tmp_path, first, "reviewer", "2026-09-28"))
    _, existing = load(tmp_path)
    inputs.en["BP02-070EN"].faces[0].text = "Changed rule."
    with pytest.raises(ValueError, match="requires re-review"):
        plan_files(tmp_path, build(inputs, existing), "reviewer", "2026-09-29")
    with pytest.raises(ValueError, match="delete"):
        plan_files(tmp_path, first[:-1], "reviewer", "2026-09-29")


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        ("Class", "Swordcraft", "Class mismatch"),
        ("Card Type", "Spell", "Type mismatch"),
    ],
)
def test_mapping_rejects_structural_conflicts(
    inputs: Inputs, field: str, value: str, message: str
) -> None:
    inputs.en["BP02-070EN"].faces[0].info[field] = value
    with pytest.raises(ValueError, match=message):
        validate_mapping(inputs.jp, inputs.en, inputs.mapping)


def test_mapping_checks_numbers_names_and_all_faces(inputs: Inputs) -> None:
    inputs.en["BP02-070EN"].faces[0].stats["power"] = "9"
    with pytest.raises(ValueError, match="power mismatch"):
        validate_mapping(inputs.jp, inputs.en, inputs.mapping)
    inputs.en["BP02-070EN"].faces[0].stats["power"] = "2"
    inputs.en["PR-001EN"] = card("PR-001EN", "Different", english=True)
    inputs.mapping.targets["PR-001EN"] = "PR-001"
    with pytest.raises(ValueError, match="English name mismatch"):
        validate_mapping(inputs.jp, inputs.en, inputs.mapping)
    inputs.en["PR-001EN"].faces[0].name = "Name"
    inputs.en["PR-001EN"].faces.append(inputs.en["PR-001EN"].faces[0].model_copy())
    with pytest.raises(ValueError, match="Face count mismatch"):
        validate_mapping(inputs.jp, inputs.en, inputs.mapping)


def test_last_confirmation_revokes_mapping_and_art(tmp_path: Path) -> None:
    candidates = tmp_path / "candidates.jsonl"
    candidates.write_text(
        json.dumps(
            {"en_no": "X-001EN", "category": "A", "jp_candidates": [{"jp_no": "X-001"}]}
        )
        + "\n"
    )
    confirmation = tmp_path / "confirmed.tsv"
    confirmation.write_text(
        "en_no\tjp_no\tverdict\tconfirmed_on\nX-001EN\t\tconfirmed_en_original_art\t2026-09-28\nX-001EN\tX-001\ten_only_reskin_same_rules\t2026-09-28\n"
    )
    mapping = read_mapping(candidates, confirmation)
    assert mapping.targets == {"X-001EN": None}
    assert mapping.original_art == set()
    assert mapping.reskins == {"X-001EN": "X-001"}


def test_duplicate_source_numbers_are_not_overwritten(tmp_path: Path) -> None:
    path = tmp_path / "cards.jsonl"
    row = card("BP01-001", "Name").model_dump_json() + "\n"
    path.write_text(row * 2)
    with pytest.raises(ValueError, match="Duplicate card number"):
        read_cards(path)


@pytest.mark.parametrize(
    "source",
    [
        "a: 1\na: 2\n",
        "a: &ref 1\nb: *ref\n",
        "true: value\n",
        "1: value\n",
        "a: !!str test\n",
        "%YAML 1.1\n---\na: yes\n",
        "a: 1\n---\na: 2\n",
        "a: .inf\n",
        "<<: value\n",
    ],
)
def test_yaml_rejects_unsafe_or_ambiguous_inputs(tmp_path: Path, source: str) -> None:
    path = tmp_path / "input.yaml"
    path.write_text(source)
    with pytest.raises((ValueError, TypeError, YAMLError)):
        read_yaml(path)


def test_yaml_core_preserves_words_and_dates(tmp_path: Path) -> None:
    path = tmp_path / "input.yaml"
    path.write_text("on: yes\nn: no\ndate: 2026-09-28\nflag: true\n")
    assert read_yaml(path) == {
        "on": "yes",
        "n": "no",
        "date": "2026-09-28",
        "flag": True,
    }


def test_yaml_size_boundary(tmp_path: Path) -> None:
    path = tmp_path / "huge.yaml"
    path.write_bytes(b" " * 1_048_576)
    with pytest.raises(ValueError, match="Oversized"):
        read_yaml(path)


def test_shard_mutation_and_interrupted_write_rejected(
    inputs: Inputs, tmp_path: Path
) -> None:
    files = plan_files(tmp_path, build(inputs, {}), "reviewer", "2026-09-28")
    write_files(files)
    orphan = tmp_path / "registry" / "card" / "unindexed.yaml"
    orphan.write_text("orphan: true\n")
    with pytest.raises(ValueError, match="closure"):
        load(tmp_path)
    orphan.unlink()
    path = next(path for path in files if "printing" in path.parts)
    path.write_text(path.read_text().replace("standard", "tampered"))
    with pytest.raises(ValueError, match="Modified immutable shard"):
        load(tmp_path)


def test_integrity_rejects_dangling_face_and_reused_integer(inputs: Inputs) -> None:
    entries = build(inputs, {})
    printing = next(entry for entry in entries if entry.kind == "printing")
    printing.data["source_face_map"] = [{"source_index": 0, "face_id": "f:" + "0" * 32}]
    with pytest.raises(ValueError, match="face mapping"):
        validate(entries)
    entries = build(inputs, {})
    allocations = [entry for entry in entries if entry.kind == "card_int_id"]
    allocations[1].data["int_id"] = allocations[0].data["int_id"]
    with pytest.raises(ValueError, match="reused int_id"):
        validate(entries)


def test_receipt_detects_changed_rules_before_allocation(
    inputs: Inputs, tmp_path: Path
) -> None:
    paths = {
        key: tmp_path / (key + ".jsonl")
        for key in ("jp", "en", "candidates", "confirmations", "original_art")
    }
    for path in paths.values():
        path.write_text("{}\n")
    inputs.receipt.input_hashes = {key: file_hash(path) for key, path in paths.items()}
    receipt = tmp_path / "receipt.json"
    receipt.write_text(inputs.receipt.model_dump_json())
    paths["en"].write_text('{"text":"Changed rules"}\n')
    with pytest.raises(ValueError, match="Unreviewed input change: en"):
        read_inputs(paths, receipt, tmp_path)


@pytest.fixture
def official_registry(
    official_snapshot: RegistrySnapshot, _official_entries: tuple[Entry, ...]
) -> list[Entry]:
    root = Path(__file__).resolve().parents[2] / "authored"
    index = official_snapshot.files.index()
    assert index.next_int_id == {"en": 67421, "jp": 27370}
    assert all(
        path.stat().st_size <= TARGET_BYTES
        for directory in (root / "ids", root / "registry")
        for path in directory.rglob("*.yaml")
    )
    assert [path.name for path in (root / "ids" / "BP01").iterdir()] == ["001.yaml"]
    return detached_entries(_official_entries)


def test_official_registry_special_mappings(official_registry: list[Entry]) -> None:
    printings = {
        (string(entry.data, "region"), string(entry.data, "card_no")): entry.data
        for entry in official_registry
        if entry.kind == "printing"
    }
    assert len(printings) == 14789
    assert sum(region == "en" for region, _ in printings) == 7420
    ids = region_allocations(official_registry)
    assert sorted(ids["jp"]) == list(range(20001, 27370))
    assert sorted(ids["en"]) == list(range(60001, 67421))
    pairs = [
        ("BP09-P50EN", "DSD01b-T01"),
        ("BP02-070EN", "BP02-071"),
        ("PCS01-001EN", "PCS02-001"),
        ("BP18-SP01EN", "BP18-081"),
        ("CP04-P06EN", "PCS01-005"),
        ("CP04-P34EN", "PCS01-019"),
        ("CP04-SL22EN", "PCS01-040"),
    ]
    for en, jp in pairs:
        assert printings["en", en]["card_id"] == printings["jp", jp]["card_id"]
    assert (
        printings["en", "BP02-070EN"]["card_id"]
        != printings["jp", "BP02-070"]["card_id"]
    )
    assert (
        printings["en", "BP03-036EN"]["card_id"]
        == printings["en", "BP03-SP01EN"]["card_id"]
    )
    for first, second in [("CP03-125", "CP03-126"), ("BP04-046", "DSD01a-013")]:
        assert printings["jp", first]["card_id"] == printings["jp", second]["card_id"]
    jp_cards = {
        entry["card_id"]
        for (region, _), entry in printings.items()
        if region == "jp" and isinstance(entry["card_id"], str)
    }
    for number in ("GFD02-003EN", "BP03-LDⓈ01EN", "PR-041EN", "BP03-054EN"):
        assert printings["en", number]["card_id"] not in jp_cards


def test_official_curation_and_corrections(official_registry: list[Entry]) -> None:
    arts = [entry for entry in official_registry if entry.kind == "art"]
    assert len(arts) == 9
    assert (
        len([entry for entry in official_registry if entry.kind == "card_related"]) == 3
    )
    assert (
        len(
            [
                entry
                for entry in official_registry
                if entry.kind == "region_mapping_review"
            ]
        )
        == 127
    )
    corrections = [
        entry for entry in official_registry if entry.kind == "source_correction"
    ]
    assert len(corrections) == 9
    active = [entry for entry in corrections if entry.data["state"] == "active"]
    assert len(active) == 9
    original = next(
        entry
        for entry in active
        if entry.data["printing_id"] == permanent_id("p", "jp:BP07-P06")
    )
    evidence: JsonValue = original.data["evidence"]
    assert isinstance(evidence, list)
    assert isinstance(evidence[0], dict)
    assert (
        evidence[0]["sha256"]
        == "sha256:1836823f9314c7bbe7c33b300d5acc99f754eb6edf566f6a69dfcb49a6bbc79b"
    )


def test_reviewed_art_group_shares_illustration_across_printings(
    inputs: Inputs,
) -> None:
    inputs.en["PR-002EN"] = card("PR-002EN", "Name", english=True)
    inputs.mapping.targets["PR-002EN"] = "BP02-071"
    inputs.mapping.original_art.add("PR-002EN")
    inputs.receipt.art_groups = [["BP02-070EN", "PR-002EN"]]
    entries = build(inputs, {})
    validate(entries)
    arts = [entry for entry in entries if entry.kind == "art"]
    assert len(arts) == 1
    uses = arts[0].data["uses"]
    assert isinstance(uses, list)
    assert len(uses) == 2
    inputs.receipt.art_groups.append(["PR-002EN"])
    with pytest.raises(ValueError, match="disjoint"):
        build(inputs, {})


@pytest.mark.parametrize("side", ["en_only", "reskin_from", "reskin_to"])
def test_append_requires_new_printing_evidence(inputs: Inputs, side: str) -> None:
    if side == "en_only":
        inputs.mapping.reskins = {}
    old = {entry.record_key: entry for entry in build(inputs, {})}
    before = {key: entry.model_dump_json() for key, entry in old.items()}
    if side == "reskin_to":
        inputs.jp["PR-002"] = card("PR-002", "名前")
    else:
        inputs.en["GF01-002EN"] = card("GF01-002EN", "Reskin", english=True)
        inputs.mapping.targets["GF01-002EN"] = None
    with pytest.raises(ValueError, match="requires re-review"):
        build(inputs, old)
    assert {key: entry.model_dump_json() for key, entry in old.items()} == before
    expanded = build(inputs, {})
    for index, entry in enumerate(expanded):
        if entry.kind == (
            "card_related" if side.startswith("reskin") else "region_mapping_review"
        ):
            expanded[index] = old[entry.record_key]
    with pytest.raises(ValueError, match="requires re-review"):
        validate(expanded)


@pytest.mark.parametrize("kind", ["region_mapping_review", "card_related"])
@pytest.mark.parametrize("damage", ["missing", "duplicate", "stale", "wrong_role"])
def test_validate_rejects_invalid_decision_evidence(
    inputs: Inputs, kind: str, damage: str
) -> None:
    entries = build(inputs, {})
    record = next(entry for entry in entries if entry.kind == kind)
    evidence = record.data[
        "observations" if kind == "region_mapping_review" else "evidence"
    ]
    assert isinstance(evidence, list)
    assert isinstance(evidence[0], dict)
    if damage == "missing":
        evidence.pop()
    elif damage == "duplicate":
        evidence.append(evidence[0])
    elif damage == "stale":
        evidence[0]["rules_hash"] = "sha256:" + "0" * 64
    else:
        evidence[0]["role"] = "invalid"
    with pytest.raises(ValueError, match="requires re-review"):
        validate(entries)


def test_pending_type_exception_requires_explicit_scope(inputs: Inputs) -> None:
    correction = Correction(
        region="en",
        card_no="BP02-070EN",
        field="card_type",
        expected_raw_value="Follower",
        corrected_value="Spell",
        image_sha256="sha256:" + "0" * 64,
        locator="Type box",
        state="needs_review",
        reason="Image-supported candidate",
    )
    inputs.receipt.corrections = [correction]
    with pytest.raises(ValueError, match="identity-only scope"):
        validation_cards(inputs)
    correction.adoption_scope = "identity_check_only"
    correction.source_correction_status = "pending_user_confirmation"
    _, checked = validation_cards(inputs)
    assert checked["BP02-070EN"].faces[0].info["Card Type"] == "Spell"
    assert inputs.en["BP02-070EN"].faces[0].info["Card Type"] == "Follower"
    assert correction.state == "needs_review"


@pytest.mark.parametrize(
    "scenario", ["applied", "already_fixed", "conflict", "needs_review"]
)
def test_correction_projection_preserves_observations(
    inputs: Inputs, scenario: str
) -> None:
    inputs.receipt.corrections = [
        Correction(
            region="jp",
            card_no="BP02-071",
            field="effect",
            expected_raw_value="Rule.",
            corrected_value="Corrected rule.",
            image_sha256="sha256:" + "0" * 64,
            locator="Rules box",
            state="needs_review" if scenario == "needs_review" else "active",
            reason="User confirmed printed text.",
        )
    ]
    entries = build(inputs, {})
    if scenario == "already_fixed":
        inputs.jp["BP02-071"].faces[0].text = "Corrected rule."
    elif scenario == "conflict":
        inputs.jp["BP02-071"].faces[0].name = "Changed source with unchanged raw effect"
    before = inputs.model_dump_json()
    projected = project_corrections(inputs, entries)
    assert inputs.model_dump_json() == before
    if scenario == "needs_review":
        assert projected == []
        return
    assert len(projected) == 1
    row = projected[0]
    assert row["card_no"] == "BP02-071"
    assert row["status"] == scenario
    if scenario == "applied":
        assert row["value"] == "Corrected rule."
        assert row["corrections"] == [
            {
                "field": "effect",
                "corrected_from": "Rule.",
                "is_corrected": True,
                "reason": "User confirmed printed text.",
            }
        ]
    else:
        assert row["corrections"] == []
    assert inputs.jp["PR-001"].faces[0].text == "Rule. (Reminder.)"


def test_confirmed_type_projection_has_field_local_badge(inputs: Inputs) -> None:
    inputs.en["GF01-001EN"].faces[0].info["Card Type"] = "Spell"
    inputs.receipt.corrections = [
        Correction(
            region="en",
            card_no="GF01-001EN",
            field="card_type",
            expected_raw_value="Spell",
            corrected_value="Follower",
            image_sha256="sha256:" + "0" * 64,
            locator="Type label",
            state="active",
            reason="User confirmed printed type.",
        )
    ]
    projected = project_corrections(inputs, build(inputs, {}))
    assert projected[0]["value"] == "Follower"
    assert projected[0]["corrections"] == [
        {
            "field": "card_type",
            "corrected_from": "Spell",
            "is_corrected": True,
            "reason": "User confirmed printed type.",
        }
    ]
    assert inputs.en["GF01-001EN"].faces[0].info["Card Type"] == "Spell"


def test_official_promotions_have_user_confirmed_decisions(
    official_snapshot: RegistrySnapshot,
) -> None:
    root = Path(__file__).resolve().parents[2] / "authored/registry/source_correction"
    assert list((root / "needs_review").rglob("*.yaml")) == []
    promoted = []
    shards = {shard.path: shard for shard in official_snapshot.files.shards}
    for path in (root / "active").rglob("*.yaml"):
        name = path.relative_to(root.parents[1]).as_posix()
        shard = shards[name].envelope()
        decision = shard.decisions[0]
        assert decision.state == "confirmed"
        if path.parent.name == "BP07":
            continue
        assert decision.reviewed_by == "user"
        assert decision.reviewed_at == "2026-09-28T00:00:00Z"
        assert decision.sample_ids == [key for key, _ in decision.members]
        promoted.extend(shard.records)
    assert len(promoted) == 8


def test_first_batch_starts_each_region_range(inputs: Inputs) -> None:
    assert int_ids(build(inputs, {})) == {
        "jp:BP02-071": 20001,
        "jp:PR-001": 20002,
        "en:BP02-070EN": 60001,
        "en:GF01-001EN": 60002,
    }


def test_allocation_order_ties_break_on_owner_raw_number_variant_and_id() -> None:
    def printing(owner: str, number: str, variant: str, identifier: str) -> Entry:
        return Entry(
            record_key="printing:" + identifier,
            kind="printing",
            owner=owner,
            data={
                "id": identifier,
                "region": "jp",
                "card_no": number,
                "variant_key": variant,
            },
        )

    rows = [
        printing("PR", "AA01-001", "standard", "p:1"),
        printing("BP01", "BP01-01a", "standard", "p:2"),
        printing("BP01", "BP01-010", "standard", "p:3"),
        printing("BP01", "BP01-010", "foil", "p:5"),
        printing("BP01", "BP01-010", "foil", "p:4"),
    ]
    ordered = sorted(rows, key=allocation_order)
    assert [string(row.data, "id") for row in ordered] == [
        "p:4",
        "p:5",
        "p:3",
        "p:2",
        "p:1",
    ]


def test_region_range_exhaustion_fails_whole_batch(
    inputs: Inputs, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setitem(allocation.REGION_RANGES, "jp", IdRange(20001, 20002))
    entries = build(inputs, {})
    assert max(int_ids(entries).values()) == 60002
    exhausted = Index(next_int_id={"en": 60003, "jp": 20003})
    assert exhausted.next_int_id["jp"] == 20003
    inputs.jp["PR-002"] = card("PR-002", "別名")
    old = {entry.record_key: entry for entry in entries}
    with pytest.raises(ValueError, match="exhausted for region jp"):
        build(inputs, old)
    with pytest.raises(ValueError, match="exhausted for region jp"):
        build(inputs, {})


@pytest.mark.parametrize(
    "cursor",
    [
        {"jp": 20001},
        {"jp": 20001, "en": 60001, "zh": 1},
        {"jp": 20000, "en": 60001},
        {"jp": 60001, "en": 60001},
        {"jp": 20001, "en": 100001},
    ],
)
def test_index_rejects_cursors_outside_policy(cursor: dict[str, int]) -> None:
    assert Index(next_int_id={"jp": 60000, "en": 100000}).next_int_id["en"] == 100000
    with pytest.raises(ValueError, match="cursors disagree"):
        Index(next_int_id=cursor)


def test_index_rejects_unknown_policy_and_format() -> None:
    with pytest.raises(ValueError, match="Unknown allocation policy"):
        Index(allocation_policy="region-ranges-v0")
    with pytest.raises(ValueError, match="authored_format"):
        Index.model_validate({"authored_format": 1, "next_int_id": 1})


@pytest.mark.parametrize("damage", ["cross_region", "reserved", "gap", "missing"])
def test_validate_rejects_bad_int_ids(inputs: Inputs, damage: str) -> None:
    entries = build(inputs, {})
    jp = next(
        entry
        for entry in entries
        if entry.record_key == "card_int_id:" + permanent_id("p", "jp:BP02-071")
    )
    if damage == "missing":
        entries.remove(jp)
        with pytest.raises(ValueError, match="coverage mismatch"):
            validate(entries)
        return
    jp.data["int_id"] = {"cross_region": 60005, "reserved": 5, "gap": 60000}[damage]
    with pytest.raises(ValueError, match="out-of-range"):
        validate(entries)


def test_region_lookup_rejects_unknown_region() -> None:
    with pytest.raises(ValueError, match="No int_id range"):
        allocation.region_range("cn")
    with pytest.raises(ValueError, match="No int_id range"):
        allocation.cursors({"cn": [1]})


def test_cursor_mismatch_and_regression_rejected(
    inputs: Inputs, tmp_path: Path
) -> None:
    entries = build(inputs, {})
    check_cursors({"en": 60003, "jp": 20003}, entries)
    with pytest.raises(ValueError, match="high-water"):
        check_cursors({"en": 60003, "jp": 20004}, entries)
    write_files(plan_files(tmp_path, entries, "reviewer", "2026-09-28"))
    index, old = load(tmp_path)
    index.next_int_id["jp"] = 20009
    inputs.jp["PR-002"] = card("PR-002", "別名")
    with pytest.raises(ValueError, match="backwards"):
        plan_files(
            tmp_path,
            build(inputs, old),
            "reviewer",
            "2026-09-29",
            loaded=(index, old),
        )


def test_shards_fill_to_target_measured_on_final_yaml(
    inputs: Inputs, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    for number in range(1, 13):
        name = f"PR-{number + 100:03}"
        inputs.jp[name] = card(name, "名前")
    entries = build(inputs, {})
    printings = sorted(
        (
            entry
            for entry in entries
            if entry.kind == "printing" and entry.owner == "PR"
        ),
        key=storage.record_order(entries),
    )
    envelope = storage._shard(printings[:4], "reviewer", "2026-09-28")
    target = len(encode(envelope))
    monkeypatch.setattr(storage, "TARGET_BYTES", target)
    files = plan_files(tmp_path, entries, "reviewer", "2026-09-28")
    shards = [
        Shard.model_validate(yaml_parser().load(data))
        for path, data in sorted(files.items())
        if path.parent == tmp_path / "registry" / "printing" / "PR"
    ]
    assert [len(shard.records) for shard in shards] == [4, 4, 4, 1]
    assert [record for shard in shards for record in shard.records] == printings
    assert all(
        len(data) <= target
        for path, data in files.items()
        if "printing" in path.parts and path.parent.name == "PR"
    )


def test_single_oversized_record_fails_clearly(
    inputs: Inputs, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(storage, "TARGET_BYTES", 10)
    monkeypatch.setattr(storage, "MAX_BYTES", 1000)
    with pytest.raises(ValueError, match="Single registry record exceeds 1 MiB"):
        plan_files(tmp_path, build(inputs, {}), "reviewer", "2026-09-28")


def test_full_relayout_reproduces_first_write(inputs: Inputs, tmp_path: Path) -> None:
    first = plan_files(tmp_path, build(inputs, {}), "reviewer", "2026-09-28")
    write_files(first)
    _, entries = load(tmp_path)
    reviews = {
        (storage._area(entry), entry.owner): ("reviewer", "2026-09-28")
        for entry in entries.values()
    }
    assert relayout(tmp_path, list(entries.values()), reviews) == first
