"""Independent indexed, immutable and confirmed adoption-envelope refusals."""

import re
import shutil
from typing import TYPE_CHECKING

import pytest

from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.wording_adoptions import loader
from sve_carddb.wording_adoptions.models import AdoptionRecord

from .test_wording_adoption_integration import adoption_case as adoption_case  # ruff: ignore[useless-import-alias] -- shared immutable synthetic repository
from .wording_adoption_fixtures import commit, install_adoptions

if TYPE_CHECKING:
    from pathlib import Path

    from .wording_adoption_fixtures import AdoptionCase


def human(record: AdoptionRecord) -> AdoptionRecord:
    wire = record.model_dump(mode="json")
    wire["data"]["review"] = {"mode": "human", "rule_set": None, "rule_matches": []}
    return AdoptionRecord.model_validate_json(canonical(wire))


def successor(record: AdoptionRecord, decision: str) -> AdoptionRecord:
    wire = record.model_dump(mode="json")
    wire["record_key"] = canonical(
        ["wording_adoption", record.data.face_id, record.data.region, 2]
    ).decode()
    wire["data"]["adoption_no"] = 2
    wire["data"]["previous"] = {
        "kind": "adoption",
        "record_key": record.record_key,
        "record_hash": digest(canonical(record.model_dump(mode="json"))),
        "decision_id": decision,
    }
    return AdoptionRecord.model_validate_json(canonical(wire))


@pytest.mark.parametrize(
    "change",
    [
        "hash",
        "members",
        "mixed",
        "rulesets",
        "approval-note",
        "duplicate-records",
        "region",
        "decision",
        "record-key",
        "inventory-link",
        "index-link",
        "sha",
    ],
)
def test_real_loader_rejects_each_independent_envelope_violation(  # ruff: ignore[complex-structure, too-many-locals, too-many-branches, too-many-statements] -- independent minimal corruption of each indexed envelope constraint
    adoption_case: AdoptionCase, tmp_path: Path, change: str
) -> None:
    case = adoption_case
    root = tmp_path / "repository"
    if change == "sha":
        with pytest.raises(
            ValueError,
            match=r"\AAdoption authored revision must be a complete Git SHA\Z",
        ):
            loader.load_adoptions(
                case.root / "authored",
                authored_revision="short",
                registry=case.scope.registry,
                stores={},
            )
        return
    shutil.copytree(case.root, root, ignore=shutil.ignore_patterns("carddb"))
    authored = root / "authored"
    first = case.replayed[0].record
    if change in {"mixed", "rulesets", "duplicate-records"}:
        second = successor(first, case.replayed[0].decision.id)
        if change == "mixed":
            second = human(second)
        elif change == "rulesets":
            pin = second.data.review.rule_set
            assert pin is not None
            second = second.model_copy(
                update={
                    "data": second.data.model_copy(
                        update={
                            "review": second.data.review.model_copy(
                                update={
                                    "rule_set": pin.model_copy(
                                        update={"policy_id": "synthetic-alternate"}
                                    )
                                }
                            )
                        }
                    )
                }
            )
        install_adoptions(authored, [first, second])
    elif change in {"decision", "record-key"}:
        install_adoptions(
            authored, [first if change == "decision" else human(first)], sequence="002"
        )
    path = authored / "wording-adoptions/jp/001.yaml"
    shard = object_value(read_yaml(path))
    decision = object_value(array(shard["decisions"])[0])
    if change == "hash":
        shard["default_decision_id"] = "d:" + "0" * 64
    elif change == "members":
        decision["sample_ids"] = []
    elif change == "approval-note":
        decision["note"] = "Synthetic note without a policy approval attribution."
    elif change == "duplicate-records":
        records = array(shard["records"])
        shard["records"] = [records[0], records[0]]
    if change in {"hash", "members", "approval-note", "duplicate-records"}:
        path.write_bytes(canonical(shard))
    index_path = authored / loader.INDEX_PATH
    index = object_value(read_yaml(index_path))
    if change != "hash":
        object_value(index["includes"])["wording-adoptions/jp/001.yaml"] = digest(
            canonical(shard)
        )
    if change == "region":
        destination = authored / "wording-adoptions/en/001.yaml"
        destination.parent.mkdir()
        path.rename(destination)
        object_value(index["includes"]).clear()
        object_value(index["includes"])["wording-adoptions/en/001.yaml"] = digest(
            canonical(shard)
        )
    index_path.write_bytes(canonical(index))
    if change in {"inventory-link", "index-link"}:
        target = path if change == "inventory-link" else index_path
        copy = root / "synthetic-copy.yaml"
        target.rename(copy)
        target.symlink_to(copy)
    revision = commit(root)
    message = {
        "hash": "Modified immutable adoption shard",
        "members": "Confirmed adoption must cover every exact record member",
        "mixed": "Human and policy reviews or distinct rule sets cannot share a shard",
        "rulesets": "Human and policy reviews or distinct rule sets cannot share a shard",
        "approval-note": "Policy review must explicitly identify policy approval",
        "duplicate-records": "Adoption records must be sorted and unique",
        "region": "Adoption record region disagrees with shard path",
        "decision": "Duplicate adoption decision",
        "record-key": "Duplicate adoption record key",
        "inventory-link": "Adoption inventory symlinks are forbidden",
        "index-link": "Adoption input symlinks are forbidden",
        "sha": "Adoption authored revision must be a complete Git SHA",
    }[change]
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        loader.load_adoptions(
            authored,
            authored_revision="short" if change == "sha" else revision,
            registry=case.scope.registry,
            stores={"wording-store": case.store},
        )


def test_absolute_input_path_is_refused_before_open(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match=r"\AUnsafe adoption input path\Z"):
        loader._file(tmp_path, str(tmp_path.parent / "outside.yaml"))


def test_numeric_aliases_cannot_duplicate_a_regional_sequence(tmp_path: Path) -> None:
    with pytest.raises(
        ValueError, match=r"\ADuplicate regional adoption shard sequence\Z"
    ):
        loader._inventory(
            tmp_path,
            {"wording-adoptions/jp/001.yaml", "wording-adoptions/jp/0001.yaml"},
        )


def test_chain_answer_must_bind_the_exact_prior_selected_observation(
    adoption_case: AdoptionCase,
) -> None:
    first = adoption_case.replayed[0].record
    decision = adoption_case.replayed[0].decision.id
    second = successor(first, decision)
    wire = second.model_dump(mode="json")
    wire["data"]["previous_order"] = {
        "basis": "reviewed_order",
        "evidence_indexes": [],
        "review_receipt": {
            "reviewed_by": "Synthetic maintainer",
            "reviewed_at": "2026-10-02T00:00:00Z",
            "reviewed_precision": "day",
            "note": "Synthetic independent successor answer",
            "before_observation_keys": ["synthetic-wrong-key"],
            "after_observation_keys": [second.data.selected_observation_key],
        },
    }
    second = AdoptionRecord.model_validate_json(canonical(wire))
    with pytest.raises(
        ValueError,
        match=r"\APrevious-order answer disagrees with both selected observations\Z",
    ):
        loader._chain([first, second], {first.record_key: decision})
