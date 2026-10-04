"""Each mutation changes one signed invariant; no official strings are fixtures."""

import copy
import re
import shutil
from typing import TYPE_CHECKING, cast

import pytest

from sve_carddb.catalog.adoption_loader import load_adoptions
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value

from .adoption_fixtures import (
    Case,
    dependency,
    envelope,
    fields,
    index,
    make_case,
    record,
    write,
)

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.catalog.adoption_loader import Entry

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
        ("duplicate", "Invalid authored YAML syntax"),
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
        ("id", "Adoption decision ID mismatch"),
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
        if mutation == "id":
            raw["default_decision_id"] = decision["id"]
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


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("hash", "Adoption shard canonical hash mismatch"),
        ("record_key", "Adoption record key does not match subject/revision"),
        ("record_order", "Adoption record keys must be sorted and unique"),
        ("filing", "Adoption kind/filing key does not match path"),
        ("kind", "Adoption kind/filing key does not match path"),
        ("dependency_fields", "Adoption dependency primary key fields mismatch"),
        ("dependency_value", "Adoption dependency keys must be nonempty text"),
        ("dependency_order", "Adoption array must be sorted and unique"),
        ("evidence_batch", "Adoption evidence batch absent from review context"),
        ("duplicate_decision", "Duplicate adoption decision"),
        ("duplicate_record", "Duplicate adoption record"),
        ("symbol_owner", "Symbol code/id must be one-to-one across history"),
        ("alias_pins", "Effective aliases cannot mix normalizer pins"),
    ],
)
def test_review_envelope_single_guard(  # ruff: ignore[complex-structure,too-many-branches] -- each signed mutation isolates one review finding
    case: Case, mutation: str, message: str
) -> None:
    area = (
        "languages"
        if mutation == "record_order"
        else "aliases"
        if mutation == "alias_pins"
        else "symbols"
    )
    path = f"catalog-adoptions/{area}/shared/001.yaml"
    raw = object_value(read_yaml(case.root / path))
    member, data, _ = fields(raw)
    members = array(raw["records"])
    if mutation == "hash":
        indexed = object_value(read_yaml(case.root / "catalog-adoptions/index.yaml"))
        object_value(indexed["includes"])[path] = "sha256:" + "f" * 64
        write(case.root, "catalog-adoptions/index.yaml", indexed)
    elif mutation == "duplicate_decision":
        write(case.root, "catalog-adoptions/symbols/shared/002.yaml", raw)
        index(case.root)
    else:
        if mutation == "record_key":
            member["record_key"] = "synthetic:wrong-key"
        elif mutation == "filing":
            member["filing_key"] = "other"
        elif mutation == "kind":
            path = "catalog-adoptions/languages/shared/001.yaml"
        elif mutation == "dependency_fields":
            object_value(object_value(array(data["dependencies"])[0])["key"])[
                "extra"
            ] = "unexpected"
        elif mutation == "dependency_value":
            object_value(object_value(array(data["dependencies"])[0])["key"])[
                "code"
            ] = ""
        elif mutation == "dependency_order":
            data["dependencies"] = [
                dependency("language", code="ja"),
                dependency("language", code="ja"),
            ]
        elif mutation == "evidence_batch":
            member["evidence"] = [
                {
                    "source_ref": {
                        "batch_id": "sha256:" + "a" * 64,
                        "source_version_id": "src:v1:" + "b" * 64,
                        "parser": "exact-json-v1",
                        "locator": "/faces/0/name",
                        "text_hash": "sha256:" + "c" * 64,
                    },
                    "role": "Synthetic evidence",
                }
            ]
        elif mutation in {"duplicate_record", "symbol_owner", "alias_pins"}:
            extra = copy.deepcopy(member)
            extra_data = object_value(extra["data"])
            subject = object_value(extra_data["subject"])
            subject["text" if mutation == "alias_pins" else "id"] = "synthetic:second"
            extra["record_key"] = canonical([extra["kind"], subject, 1]).decode()
            if mutation == "alias_pins":
                object_value(object_value(extra_data["value"])["normalizer"])[
                    "version"
                ] = "synthetic-other"
            elif mutation == "duplicate_record":
                object_value(extra_data["value"])["code"] = "another-code"
                path = "catalog-adoptions/symbols/shared/002.yaml"
            members.append(extra)
        rewritten = envelope(members, case.review)
        if mutation == "kind":
            decision = object_value(array(rewritten["decisions"])[0])
            decision["category"] = "language_adoption"
            decision["policy_id"] = "catalog-language-v1"
        if mutation == "record_order":
            rewritten["records"] = list(reversed(array(rewritten["records"])))
        write(case.root, path, rewritten)
        index(case.root)
    with pytest.raises(ValueError, match=rf"^{re.escape(message)}$"):
        load_adoptions(case.root, entry="catalog-adoptions")


def test_unknown_entry(case: Case) -> None:
    with pytest.raises(ValueError, match=r"^Unknown adoption entry$"):
        load_adoptions(case.root, entry=cast("Entry", "other"))


@pytest.mark.parametrize("indexed", [False, True])
def test_symlink_entry_and_unindexed_file(case: Case, indexed: bool) -> None:
    path = (
        case.root / "catalog-adoptions/index.yaml"
        if indexed
        else case.root / "catalog-adoptions/unindexed.txt"
    )
    if indexed:
        path.rename(case.root / "original-index")
    path.symlink_to(case.root / "original-index")
    with pytest.raises(ValueError, match=r"^Symlink adoption input$"):
        load_adoptions(case.root, entry="catalog-adoptions")


def test_symlink_parent_directory(case: Case) -> None:
    target = case.root.with_name("original-authored")
    case.root.rename(target)
    case.root.symlink_to(target, target_is_directory=True)
    with pytest.raises(ValueError, match=r"^Symlink adoption input$"):
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
