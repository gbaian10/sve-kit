"""Each mutation changes one signed invariant; no official strings are fixtures."""

import copy
import shutil
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_loader import load_adoptions
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value

from .adoption_fixtures import Case, envelope, fields, index, make_case, record, write

if TYPE_CHECKING:
    from pathlib import Path

from pydantic import JsonValue


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> Case:
    return make_case(tmp_path_factory.mktemp("adoption") / "repository")


@pytest.fixture
def case(tmp_path: Path, baseline: Case) -> Case:

    shutil.copytree(baseline.root, tmp_path / "authored")
    return Case(
        baseline.repository,
        tmp_path / "authored",
        copy.deepcopy(baseline.review),
        copy.deepcopy(baseline.normalizer),
        baseline.revision,
    )


def test_complete_entry_and_explicit_empty(case: Case) -> None:
    snapshot = load_adoptions(case.root, entry="catalog-adoptions")
    assert len(snapshot.effective()) == 6
    index(case.root, entry="display-overrides")
    assert not load_adoptions(case.root, entry="display-overrides").effective()


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("index", "Missing or unsafe adoption input"),
        ("missing", "file closure"),
        ("unindexed", "file closure"),
        ("symlink", "Symlink"),
        ("cross_entry", "cross-entry"),
        ("unknown", "Invalid adoption fields"),
        ("duplicate", "Duplicate"),
        ("bool_format", "integer one"),
        ("sequence", "sequence"),
        ("absolute", "cross-entry"),
    ],
)
def test_file_boundary_guards(case: Case, mutation: str, message: str) -> None:
    path = case.root / "catalog-adoptions/symbols/shared/001.yaml"
    if mutation == "index":
        (case.root / "catalog-adoptions/index.yaml").unlink()
    elif mutation == "missing":
        path.unlink()
    elif mutation == "unindexed":
        path.with_name("002.yaml").write_bytes(path.read_bytes())
    elif mutation == "symlink":
        path.rename(path.with_suffix(".bak"))
        path.symlink_to(path.with_suffix(".bak"))
    elif mutation == "duplicate":
        path.write_bytes(b"catalog_adoption_format: 1\ncatalog_adoption_format: 1\n")
    elif mutation == "sequence":
        path.rename(path.with_name("002.yaml"))
        index(case.root)
    elif mutation in {"cross_entry", "absolute"}:
        raw = object_value(read_yaml(case.root / "catalog-adoptions/index.yaml"))
        includes = object_value(raw["includes"])
        value = includes.pop("catalog-adoptions/symbols/shared/001.yaml")
        includes[
            "display-overrides/symbols/shared/001.yaml"
            if mutation == "cross_entry"
            else str(path)
        ] = value
        write(case.root, "catalog-adoptions/index.yaml", raw)
    else:
        raw = object_value(read_yaml(path))
        raw["extra"] = True if mutation == "unknown" else None
        if mutation == "bool_format":
            raw.pop("extra")
            raw["catalog_adoption_format"] = True
        write(case.root, "catalog-adoptions/symbols/shared/001.yaml", raw)
        index(case.root)
    with pytest.raises(ValueError, match=message):
        load_adoptions(case.root, entry="catalog-adoptions")


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("value", "exact decision members"),
        ("evidence", "exact decision members"),
        ("member", "exact decision members"),
        ("context", "review context hash"),
        ("category", "category/policy"),
        ("policy", "category/policy"),
        ("member_missing", "exact decision members"),
        ("member_extra", "exact decision members"),
        ("member_duplicate", "exact decision members"),
        ("membership_hash", "membership hash"),
        ("id", "decision ID"),
        ("default", "default decision ID"),
        ("checked", "checked members"),
        ("proposed", "Invalid adoption fields"),
        ("sampled", "Invalid adoption fields"),
        ("reviewer", "Invalid adoption fields"),
        ("time", "Invalid adoption fields"),
        ("policy_exception", "Invalid adoption fields"),
        ("unknown_data", "Invalid adoption fields"),
    ],
)
def test_exact_member_and_receipt_guards(  # ruff: ignore[complex-structure,too-many-branches] -- independent mutations of the same minimal envelope
    case: Case, mutation: str, message: str
) -> None:
    name = "catalog-adoptions/symbols/shared/001.yaml"
    raw = object_value(read_yaml(case.root / name))
    member, data, decision = fields(raw)
    if mutation == "value":
        object_value(object_value(data["value"])["source_localization"])["name"] = {
            "kind": "authored",
            "lang": "ja",
            "text": "Changed synthetic",
        }
    elif mutation == "evidence":
        data["reason"] = "Changed evidence rationale."
    elif mutation == "member":
        extra = copy.deepcopy(member)
        extra_data = object_value(extra["data"])
        extra_data["subject"] = {"id": "symbol:another"}
        extra["record_key"] = canonical(
            ["text_symbol_adoption", extra_data["subject"], 1]
        ).decode()
        raw["records"] = sorted(
            [member, extra], key=lambda item: str(object_value(item)["record_key"])
        )
    elif mutation == "context":
        object_value(object_value(raw["review_context"])["context"])[
            "configuration"
        ] = '{"changed":true}'
    elif mutation in {"category", "policy"}:
        decision["category" if mutation == "category" else "policy_id"] = (
            "vocabulary_adoption" if mutation == "category" else "catalog-vocabulary-v1"
        )
    elif mutation.startswith("member_"):
        members = array(decision["members"])
        decision["members"] = (
            []
            if mutation == "member_missing"
            else members
            + (
                members
                if mutation == "member_duplicate"
                else [["extra", "sha256:" + "e" * 64]]
            )
        )
    elif mutation == "membership_hash":
        decision["membership_hash"] = "sha256:" + "e" * 64
    elif mutation in {"id", "default"}:
        (decision if mutation == "id" else raw)[
            "id" if mutation == "id" else "default_decision_id"
        ] = "d:" + "e" * 64
    elif mutation == "checked":
        decision["sample_ids"] = []
    elif mutation in {"proposed", "sampled"}:
        decision["state"] = mutation
    elif mutation in {"reviewer", "time"}:
        decision["reviewed_by" if mutation == "reviewer" else "reviewed_at"] = (
            "" if mutation == "reviewer" else None
        )
    elif mutation == "policy_exception":
        raw["adoption_review"] = {"mode": "approved_policy"}
    else:
        data["hidden"] = True
    write(case.root, name, raw)
    index(case.root)
    with pytest.raises(ValueError, match=message):
        load_adoptions(case.root, entry="catalog-adoptions")


def test_withdraw_restore_and_exact_predecessor(case: Case) -> None:
    first = object_value(
        read_yaml(case.root / "catalog-adoptions/symbols/shared/001.yaml")
    )
    previous, data, decision = fields(first)
    original = copy.deepcopy(data["value"])
    subject = object_value(data["subject"])
    for number, value in ((2, None), (3, original)):
        successor = record(
            "text_symbol_adoption",
            subject,
            value,
            case.review,
            array(data["dependencies"]) if value is not None else [],
            number=number,
            previous={
                "record_key": previous["record_key"],
                "record_hash": digest(canonical(previous)),
                "decision_id": decision["id"],
            },
        )
        shard = envelope([successor], case.review)
        write(case.root, f"catalog-adoptions/symbols/shared/{number:03}.yaml", shard)
        previous, data, decision = fields(shard)
    index(case.root)
    terminal = [
        r
        for r, _ in load_adoptions(case.root, entry="catalog-adoptions").effective()
        if r.kind == "text_symbol_adoption"
    ]
    assert terminal[0].data.adoption_no == 3


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("hash", "predecessor"),
        ("decision", "predecessor"),
        ("gap", "sequence"),
        ("code", "stable code"),
    ],
)
def test_revision_single_guard(case: Case, change: str, message: str) -> None:
    raw = object_value(
        read_yaml(case.root / "catalog-adoptions/symbols/shared/001.yaml")
    )
    previous, data, decision = fields(raw)
    reference: dict[str, JsonValue] = {
        "record_key": previous["record_key"],
        "record_hash": digest(canonical(previous)),
        "decision_id": decision["id"],
    }
    if change in {"hash", "decision"}:
        reference["record_hash" if change == "hash" else "decision_id"] = (
            "sha256:" + "e" * 64 if change == "hash" else "d:" + "e" * 64
        )
    value = copy.deepcopy(data["value"])
    if change == "code":
        object_value(value)["code"] = "changed"
    successor = record(
        "text_symbol_adoption",
        object_value(data["subject"]),
        value,
        case.review,
        array(data["dependencies"]),
        number=3 if change == "gap" else 2,
        previous=reference,
    )
    write(
        case.root,
        "catalog-adoptions/symbols/shared/002.yaml",
        envelope([successor], case.review),
    )
    index(case.root)
    with pytest.raises(ValueError, match=message):
        load_adoptions(case.root, entry="catalog-adoptions")
