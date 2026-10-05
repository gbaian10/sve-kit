"""Every rejection uses one anchored boundary message, never a disjunction."""

import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.digital_links.loader import decision_id, link_id, load_links
from sve_carddb.digital_links.models import Record
from sve_carddb.snapshot.values import array, canonical, object_value

from .digital_link_fixtures import envelope, record, write

if TYPE_CHECKING:
    from pathlib import Path

SHARD = "digital-links/links/synthetic/001.yaml"


def test_current_records_have_stable_subject_ids(tmp_path: Path) -> None:
    first = record()
    second = record()
    object_value(second["subject"])["official_id"] = "22345679"
    write(
        tmp_path,
        {
            SHARD: envelope([first]),
            "digital-links/links/synthetic/002.yaml": envelope([second]),
        },
    )
    snapshot = load_links(tmp_path)
    assert len(snapshot.records()) == 2
    original = Record.model_validate_json(canonical(first))
    changed = record()
    object_value(changed["value"])["relation"] = "same_character"
    revised = Record.model_validate_json(canonical(changed))
    assert link_id(original) == link_id(revised)
    assert decision_id(original) != decision_id(revised)
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
        ("decisions", "Invalid digital-link fields"),
        ("bool-format", "Invalid digital-link fields"),
        ("old-format", "Invalid digital-link fields"),
        ("withdrawn", "Invalid digital-link fields"),
        ("machine-review", "Invalid digital-link fields"),
        ("one-face", "Invalid digital-link fields"),
        ("id-width", "Invalid digital-link fields"),
        ("super-phase", "Invalid digital-link fields"),
        ("reason", "Invalid digital-link fields"),
        ("no-ja", "Digital name evidence requires Japanese for every phase"),
        ("wrong-face", "Digital-link evidence differs from declared faces"),
        ("wrong-phase", "Digital-link evidence differs from declared faces"),
    ],
)
def test_record_refusals(  # ruff: ignore[complex-structure,too-many-branches] -- each bad input targets one exact refusal
    tmp_path: Path, fault: str, message: str
) -> None:
    r = record()
    subject = object_value(r["subject"])
    value = object_value(r["value"])
    if fault == "unknown":
        r["confidence"] = "high"
    elif fault == "withdrawn":
        r["value"] = None
    elif fault == "machine-review":
        r["review_level"] = "model_reviewed"
    elif fault == "one-face":
        subject["face_id"] = None
    elif fault == "id-width":
        subject["official_id"] = "123"
    elif fault == "super-phase":
        subject["digital_phase"] = "super_evolved"
    elif fault == "reason":
        r["reason"] = "   "
    elif fault == "no-ja":
        object_value(array(value["digital_names"])[0])["lang"] = "en"
    elif fault == "wrong-face":
        object_value(array(value["sve_names"])[0])["face_id"] = "f:" + "a" * 32
    elif fault == "wrong-phase":
        object_value(array(value["digital_names"])[0])["phase"] = "evolved"
    shard = envelope([r])
    if fault == "decisions":
        shard["decisions"] = []
    elif fault == "bool-format":
        shard["digital_link_authored_format"] = True
    elif fault == "old-format":
        shard["digital_link_authored_format"] = 1
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
        ("duplicate_record", "Duplicate digital-link record"),
        ("relation_change", "Duplicate digital-link record"),
        ("duplicate_sve", "SVE name evidence must be sorted and unique"),
        (
            "duplicate_language",
            "Digital name phases and languages must be sorted and unique",
        ),
    ],
)
def test_one_relation_per_subject(tmp_path: Path, fault: str, message: str) -> None:
    first = record()
    records = [first]
    value = object_value(first["value"])
    if fault == "duplicate_record":
        records *= 2
    elif fault == "relation_change":
        other = record()
        object_value(other["value"])["relation"] = "name_only"
        records.append(other)
    elif fault == "duplicate_sve":
        value["sve_names"] = array(value["sve_names"]) * 2
    elif fault == "duplicate_language":
        value["digital_names"] = array(value["digital_names"]) * 2
    write(tmp_path, {SHARD: envelope(records)})
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
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
