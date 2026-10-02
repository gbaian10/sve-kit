"""Every rejection uses one anchored boundary message, never a disjunction."""

import re
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.digital_links.loader import key, link_id, load_links, record_hash
from sve_carddb.digital_links.models import Record
from sve_carddb.snapshot.values import array, canonical, object_value

from .digital_link_fixtures import decision, envelope, record, write

if TYPE_CHECKING:
    from pathlib import Path

SHARD = "digital-links/links/synthetic/001.yaml"


def test_sampled_and_terminal_history(tmp_path: Path) -> None:
    first = record()
    first_shard = envelope([first])
    second = record(
        number=2,
        previous={
            "record_key": first["record_key"],
            "record_hash": record_hash(Record.model_validate_json(canonical(first))),
            "decision_id": decision(first_shard)["id"],
        },
    )
    data = object_value(second["data"])
    data["value"] = None
    second["evidence"] = []
    write(
        tmp_path,
        {
            SHARD: first_shard,
            "digital-links/links/synthetic/002.yaml": envelope([second]),
        },
    )
    snapshot = load_links(tmp_path)
    assert len(snapshot.records()) == 2
    latest, _ = snapshot.effective()[0]
    assert latest.data.value is None
    original = Record.model_validate_json(canonical(first))
    assert link_id(original) == link_id(latest)
    assert key(original) != key(latest)
    assert snapshot.pins()["index_hash"]


def test_missing_and_explicit_empty(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match=r"^Missing digital-link input$"):
        load_links(tmp_path)
    write(tmp_path, {})
    assert load_links(tmp_path).records() == ()


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("unknown", "Invalid digital-link fields"),
        ("bool-format", "Invalid digital-link fields"),
        ("bool-revision", "Invalid digital-link fields"),
        ("policy", "Invalid digital-link fields"),
        ("category", "Invalid digital-link fields"),
        ("proposed", "Invalid digital-link fields"),
        ("machine-state", "Invalid digital-link fields"),
        ("reviewer", "Invalid digital-link fields"),
        ("day", "Invalid digital-link fields"),
        ("one-face", "Invalid digital-link fields"),
        ("id-width", "Invalid digital-link fields"),
        ("super-phase", "Invalid digital-link fields"),
        ("reason", "Invalid digital-link fields"),
        ("filing", "Digital-link record key or filing mismatch"),
        ("key", "Digital-link record key or filing mismatch"),
        ("review-hash", "Digital-link review context hash mismatch"),
        ("evidence", "Digital-link evidence set differs from name references"),
        ("batch", "Digital-link evidence batch absent from review context"),
        ("no-ja", "Digital name evidence requires Japanese for every phase"),
        ("wrong-face", "Digital-link evidence differs from declared faces"),
        ("wrong-phase", "Digital-link evidence differs from declared faces"),
        (
            "first-null",
            "Initial digital-link adoption must have value and no predecessor",
        ),
        ("gap", "Digital-link adoption sequence gap or fork"),
    ],
)
def test_record_refusals(  # ruff: ignore[complex-structure,too-many-branches,too-many-statements] -- each independently signed bad input targets one exact refusal
    tmp_path: Path, fault: str, message: str
) -> None:
    r = record()
    data = object_value(r["data"])
    subject = object_value(data["subject"])
    value = object_value(data["value"])
    if fault == "unknown":
        r["confidence"] = "high"
    elif fault == "bool-revision":
        data["adoption_no"] = True
    elif fault == "one-face":
        subject["face_id"] = None
    elif fault == "id-width":
        subject["official_id"] = "123"
    elif fault == "super-phase":
        subject["digital_phase"] = "super_evolved"
    elif fault == "reason":
        data["reason"] = "   "
    elif fault == "filing":
        r["filing_key"] = "other"
    elif fault == "key":
        r["record_key"] = "arbitrary"
    elif fault == "review-hash":
        data["review_context_hash"] = "sha256:" + "c" * 64
    elif fault == "evidence":
        r["evidence"] = []
    elif fault == "batch":
        for e in array(r["evidence"]):
            object_value(object_value(e)["source_ref"])["store_id"] = "other"
        for names in ("sve_names", "digital_names"):
            for e in array(value[names]):
                object_value(object_value(e)["name_ref"])["store_id"] = "other"
    elif fault == "no-ja":
        object_value(array(value["digital_names"])[0])["lang"] = "en"
    elif fault == "wrong-face":
        object_value(array(value["sve_names"])[0])["face_id"] = "f:" + "a" * 32
    elif fault == "wrong-phase":
        object_value(array(value["digital_names"])[0])["phase"] = "evolved"
    elif fault == "first-null":
        data["value"] = None
        r["evidence"] = []
    elif fault == "gap":
        data["adoption_no"] = 2
        r["record_key"] = canonical(["digital_link_adoption", subject, 2]).decode()
    shard = envelope([r])
    d = decision(shard)
    if fault == "bool-format":
        shard["digital_link_authored_format"] = True
    elif fault == "policy":
        d["policy_id"] = "glossary-initial-51-v1"
    elif fault == "category":
        d["category"] = "digital_link_adoption"
    elif fault == "proposed":
        d["state"] = "proposed"
    elif fault == "machine-state":
        d["state"] = "model_reviewed"
    elif fault == "reviewer":
        d["reviewed_by"] = None
    elif fault == "day":
        d["reviewed_at"] = "2026-10-02T00:01:00Z"
    write(tmp_path, {SHARD: shard})
    with pytest.raises(ValueError, match=r"^" + message + "$"):
        load_links(tmp_path)


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("members", "Digital-link decision members mismatch"),
        ("hash", "Digital-link membership hash mismatch"),
        ("id", "Digital-link decision ID mismatch"),
        ("default", "Digital-link default decision mismatch"),
        ("empty-sample", "Digital-link decision requires actual checked members"),
        ("foreign-sample", "Digital-link decision requires actual checked members"),
        ("duplicate-sample", "Digital-link decision requires actual checked members"),
    ],
)
def test_decision_refusals(tmp_path: Path, fault: str, message: str) -> None:
    shard = envelope([record()])
    d = decision(shard)
    if fault == "members":
        d["members"] = []
    elif fault == "hash":
        d["membership_hash"] = "sha256:" + "d" * 64
    elif fault == "id":
        d["id"] = "d:" + "d" * 64
    elif fault == "default":
        shard["default_decision_id"] = "d:" + "d" * 64
    elif fault == "empty-sample":
        d["sample_ids"] = []
    elif fault == "foreign-sample":
        d["sample_ids"] = ["other"]
    elif fault == "duplicate-sample":
        d["sample_ids"] = array(d["sample_ids"]) * 2
    write(tmp_path, {SHARD: shard})
    with pytest.raises(ValueError, match=r"^" + message + "$"):
        load_links(tmp_path)


@pytest.mark.parametrize(
    ("name", "message"),
    [
        ("../other/001.yaml", "Unsafe digital-link include"),
        ("translations/glossary/synthetic/001.yaml", "Unsafe digital-link include"),
        (
            "digital-links/coverage/synthetic/001.yaml",
            "Digital coverage adoption is not supported",
        ),
        ("digital-links/links/synthetic/002.yaml", "Digital-link shard sequence gap"),
    ],
)
def test_include_refusals(tmp_path: Path, name: str, message: str) -> None:
    write(tmp_path, {name: envelope([record()])})
    with pytest.raises(ValueError, match=r"^" + message + "$"):
        load_links(tmp_path)


def test_unindexed_and_hash(tmp_path: Path) -> None:
    write(tmp_path, {SHARD: envelope([record()])})
    extra = tmp_path / "digital-links/unindexed.json"
    extra.write_text("{}")
    with pytest.raises(
        ValueError, match=r"^Digital-link indexed file closure differs from disk$"
    ):
        load_links(tmp_path)
    extra.unlink()
    (tmp_path / SHARD).write_text("{}")
    with pytest.raises(ValueError, match=r"^Digital-link shard hash mismatch$"):
        load_links(tmp_path)


def test_symlink_even_unindexed(tmp_path: Path) -> None:
    write(tmp_path, {})
    (tmp_path / "digital-links/symlink").symlink_to(tmp_path, target_is_directory=True)
    with pytest.raises(ValueError, match=r"^Symlink digital-link input$"):
        load_links(tmp_path)


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("duplicate_record", "Digital-link members must be sorted and unique"),
        ("duplicate_sve", "SVE name evidence must be sorted and unique"),
        (
            "duplicate_language",
            "Digital name phases and languages must be sorted and unique",
        ),
        ("duplicate_evidence", "Adoption array must be sorted and unique"),
        ("confirmed_subset", "Confirmed digital-link decision must check every member"),
        (
            "initial_predecessor",
            "Initial digital-link adoption must have value and no predecessor",
        ),
    ],
)
def test_duplicates_and_receipt_scope(tmp_path: Path, fault: str, message: str) -> None:
    first = record()
    records = [first]
    value = object_value(object_value(first["data"])["value"])
    if fault == "duplicate_record":
        records *= 2
    elif fault == "duplicate_sve":
        value["sve_names"] = array(value["sve_names"]) * 2
    elif fault == "duplicate_language":
        value["digital_names"] = array(value["digital_names"]) * 2
    elif fault == "duplicate_evidence":
        first["evidence"] = array(first["evidence"]) * 2
    elif fault == "confirmed_subset":
        other = record()
        subject = object_value(object_value(other["data"])["subject"])
        subject["official_id"] = "22345679"
        other["record_key"] = canonical(["digital_link_adoption", subject, 1]).decode()
        records.append(other)
    elif fault == "initial_predecessor":
        object_value(first["data"])["predecessor"] = {
            "record_key": "missing",
            "record_hash": "sha256:" + "a" * 64,
            "decision_id": "d:" + "a" * 64,
        }
    shard = envelope(records)
    if fault == "confirmed_subset":
        decision(shard)["state"] = "confirmed"
    write(tmp_path, {SHARD: shard})
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        load_links(tmp_path)


@pytest.mark.parametrize("fault", ["missing", "key", "hash", "decision"])
def test_predecessor_is_exact(tmp_path: Path, fault: str) -> None:
    first = record()
    initial = envelope([first])
    predecessor: dict[str, JsonValue] = {
        "record_key": first["record_key"],
        "record_hash": record_hash(Record.model_validate_json(canonical(first))),
        "decision_id": decision(initial)["id"],
    }
    if fault == "key":
        predecessor["record_key"] = "other"
    elif fault == "hash":
        predecessor["record_hash"] = "sha256:" + "e" * 64
    elif fault == "decision":
        predecessor["decision_id"] = "d:" + "e" * 64
    second = record(number=2, previous=None if fault == "missing" else predecessor)
    write(
        tmp_path,
        {SHARD: initial, "digital-links/links/synthetic/002.yaml": envelope([second])},
    )
    with pytest.raises(ValueError, match=r"^Digital-link predecessor mismatch$"):
        load_links(tmp_path)


@pytest.mark.parametrize("field", ["authored_by", "reviewed_by"])
def test_blank_human_receipt_reaches_validator(tmp_path: Path, field: str) -> None:
    shard = envelope([record()])
    decision(shard)[field] = "   "
    write(tmp_path, {SHARD: shard})
    with pytest.raises(ValueError, match=r"^Invalid digital-link fields$"):
        load_links(tmp_path)


@pytest.mark.parametrize(
    "reviewer", ["Codex gpt-6.1-sol", "Synthetic tool", " gbaian10"]
)
def test_confirmer_is_exact_listed_maintainer(tmp_path: Path, reviewer: str) -> None:
    shard = envelope([record()])
    decision(shard)["reviewed_by"] = reviewer
    write(tmp_path, {SHARD: shard})
    with pytest.raises(
        ValueError,
        match=r"^Digital-link confirmer must be a repository-listed maintainer$",
    ):
        load_links(tmp_path)


@pytest.mark.parametrize("fault", ["index", "parent"])
def test_index_symlink_rejected_before_directory_scan(
    tmp_path: Path, fault: str
) -> None:
    root = tmp_path / "authored"
    write(root, {SHARD: envelope([record()])})
    if fault == "index":
        path = root / "digital-links/index.yaml"
        target = tmp_path / "index-target.yaml"
        path.rename(target)
        path.symlink_to(target)
        target.write_text("{")
    else:
        linked = tmp_path / "linked-authored"
        linked.symlink_to(root, target_is_directory=True)
        root = linked
    with pytest.raises(ValueError, match=r"^Symlink digital-link input$"):
        load_links(root)


def test_duplicate_record_across_separately_valid_shards(tmp_path: Path) -> None:
    shard = envelope([record()])
    write(tmp_path, {SHARD: shard, "digital-links/links/synthetic/002.yaml": shard})
    with pytest.raises(ValueError, match=r"^Duplicate digital-link record$"):
        load_links(tmp_path)


def test_withdrawal_cannot_carry_name_evidence(tmp_path: Path) -> None:
    r = record()
    object_value(r["data"])["value"] = None
    write(tmp_path, {SHARD: envelope([r])})
    with pytest.raises(
        ValueError, match=r"^Withdrawn digital link must have empty evidence$"
    ):
        load_links(tmp_path)
