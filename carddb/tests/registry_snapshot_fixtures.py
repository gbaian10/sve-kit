"""Synthetic full-registry files and isolated tampering helpers."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.registry.build import build
from sve_carddb.registry.inputs import digest
from sve_carddb.registry.review import Correction
from sve_carddb.registry.storage import (
    Entry,
    Index,
    Shard,
    encode,
    member_hash,
    members,
    plan_files,
    read_yaml,
    write_files,
)

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from sve_carddb.registry.review import Inputs


@pytest.fixture
def registry_root(inputs: Inputs, tmp_path: Path) -> Path:
    inputs.receipt.corrections = [
        Correction(
            region="jp",
            card_no="BP02-071",
            field="effect",
            expected_raw_value="Rule.",
            corrected_value="Corrected synthetic rule.",
            image_sha256="sha256:" + "1" * 64,
            locator="effect",
            state="active",
            reason="Synthetic correction",
        ),
        Correction(
            region="en",
            card_no="GF01-001EN",
            field="effect",
            expected_raw_value="Rule.",
            corrected_value="Candidate synthetic rule.",
            image_sha256="sha256:" + "2" * 64,
            locator="effect",
            state="needs_review",
            reason="Synthetic candidate",
        ),
    ]
    write_files(plan_files(tmp_path, build(inputs, {}), "reviewer", "2026-09-28"))
    return tmp_path


def rewrite(root: Path, path: Path, shard: Shard, *, resign: bool = False) -> None:
    if resign and shard.decisions:
        decision = shard.decisions[0]
        decision.members = members(shard.records)
        decision.membership_hash = member_hash(decision.members)
        decision.id = "d:" + decision.membership_hash.removeprefix("sha256:")
        decision.sample_ids = (
            [key for key, _ in decision.members]
            if decision.state == "confirmed"
            else []
        )
        shard.default_decision_id = decision.id
    path.write_bytes(encode(shard))
    index_path = root / "ids/index.yaml"
    index = Index.model_validate(read_yaml(index_path))
    index.includes[path.relative_to(root).as_posix()] = digest(read_yaml(path))
    index_path.write_bytes(encode(index))


def edit_record(
    root: Path, kind: str, edit: Callable[[Entry], None], *, region: str | None = None
) -> None:
    index = Index.model_validate(read_yaml(root / "ids/index.yaml"))
    for name in index.includes:
        path = root / name
        shard = Shard.model_validate(read_yaml(path))
        for entry in shard.records:
            if entry.kind == kind and (
                region is None or entry.data.get("region") == region
            ):
                edit(entry)
                rewrite(root, path, shard, resign=True)
                return
    raise AssertionError("Missing synthetic test record")


def decision_shard(root: Path, *, proposed: bool = False) -> tuple[Path, Shard]:
    index = Index.model_validate(read_yaml(root / "ids/index.yaml"))
    for name in index.includes:
        path = root / name
        shard = Shard.model_validate(read_yaml(path))
        if shard.decisions and shard.decisions[0].state == (
            "proposed" if proposed else "confirmed"
        ):
            return path, shard
    raise AssertionError("Missing synthetic test decision")
