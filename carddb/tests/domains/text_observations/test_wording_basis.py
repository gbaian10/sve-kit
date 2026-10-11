"""Semantic selection, chronology and stale decisions use synthetic source text."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.domains.text_observations.equivalence import (
    WordingDecision,
    comparison_hash,
    comparison_text,
    load_decisions,
)
from sve_carddb.domains.text_observations.wording import populate_wording_basis

from ...support.source_correction_fixtures import make_correction_case
from ...support.text_observation_fixtures import make_case
from ..registry.test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture
from .test_wording_pending import product

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.registry.review import Inputs


@pytest.mark.parametrize(
    ("left", "right"),
    [
        ("【疾走】（【疾走】を持つフォロワーは攻撃できる）", "【疾走】"),
        ("Rush. (Followers with Rush can attack followers.)", "Rush."),
        ("Draw a card, then discard a card.", "Draw a card; then discard a card."),
    ],
)
def test_only_reminders_and_sentence_punctuation_are_ignored(
    left: str, right: str
) -> None:
    assert comparison_text(left) == comparison_text(right)


@pytest.mark.parametrize(
    ("left", "right"),
    [
        ("Gain +1.", "Gain -1."),
        ("Draw 1 card.", "Draw 2 cards."),
        ("【ファンファーレ】（条件を満たすなら）1枚引く", "【ファンファーレ】1枚引く"),
        ("Select a follower (only in your cemetery).", "Select a follower."),
    ],
)
def test_rule_conditions_numbers_and_signs_are_not_reminders(
    left: str, right: str
) -> None:
    assert comparison_text(left) != comparison_text(right)


@pytest.mark.parametrize(
    "mode",
    [
        "ai",
        "manual",
        "stale",
        "same_day",
        "undated",
        "errata",
        "manual_reject_auto",
        "manual_same_day",
    ],
)
def test_semantic_basis_requires_equivalence_and_unique_dated_content(
    tmp_path: Path,
    inputs: Inputs,
    mode: str,
) -> None:
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    group = case.plan.groups[0]
    contents = tuple(item.content for item in group.observations)
    decision = WordingDecision(
        face_id=group.face_id,
        region="jp",
        comparison_hash=comparison_hash(contents),
        classification="errata"
        if mode in {"errata", "manual_reject_auto"}
        else "same_effect",
        method="manual"
        if mode in {"manual", "manual_reject_auto", "manual_same_day"}
        else "ai",
        reason="Synthetic reviewed equivalence",
        basis_printing_id=group.observations[0].printing_id
        if mode in {"manual", "manual_same_day"}
        else None,
    )
    if mode == "stale":
        decision = decision.model_copy(update={"comparison_hash": "sha256:" + "0" * 64})
    with create_database(compile_build()) as db:
        with db.transaction():
            case.stage(db)
        case.compose().import_into(
            db, build=case.context(), vocabulary=case.vocabulary, published=()
        )
        for i, item in enumerate(group.observations):
            if mode in {"undated", "manual_reject_auto"} and i == 1:
                continue
            product(
                db,
                item.printing_id,
                "2020-01-01"
                if i == 0 or mode in {"same_day", "manual_same_day"}
                else "2026-01-01",
            )
        with db.transaction():
            for row in db.rows("face_wording"):
                db.delete(
                    "face_wording",
                    {key: row.values[key] for key in ("face_id", "region")},
                )
            plan = replace(case.plan, wording_decisions=(decision,))
            if mode == "stale":
                with pytest.raises(ValueError, match="Stale wording decision"):
                    populate_wording_basis(db, plan)
                return
            populate_wording_basis(db, plan)
        assessment = db.rows("face_wording")[0].values
        if mode in {"same_day", "errata", "manual_reject_auto"}:
            assert not db.rows("face_current")
            assert assessment["classification"] == (
                "unresolved" if mode == "same_day" else "errata"
            )
        else:
            assert len(db.rows("face_current")) == 1
            expected = group.observations[
                0 if mode in {"manual", "undated", "manual_same_day"} else 1
            ]
            assert assessment["basis_printing_id"] == expected.printing_id
            assert assessment["classification"] == "same_effect"


def test_authored_decisions_are_optional_and_duplicates_are_rejected(
    tmp_path: Path,
) -> None:
    assert load_decisions(tmp_path) == ()
    target = tmp_path / "wording" / "SYNTHETIC" / "001.yaml"
    target.parent.mkdir(parents=True)
    record = (
        "  - face_id: f:synthetic\n    region: jp\n    comparison_hash: 'sha256:"
        + "0" * 64
        + "'\n    classification: pending_review\n    method: manual\n    reason: Synthetic review\n"
    )
    target.write_text(
        "format: 1\nkind: wording_decisions\nrecords:\n" + record + record
    )
    with pytest.raises(ValueError, match="Duplicate wording decision"):
        load_decisions(tmp_path)


def test_undated_unreviewed_correction_does_not_block_a_dated_basis(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_correction_case(tmp_path, inputs, state="needs_review").texts
    group = next(group for group in case.plan.groups if group.region == "jp")
    known = next(item for item in group.observations if item.card_no == "PR-001")
    with create_database(compile_build(("en", "related", "correction"))) as db:
        with db.transaction():
            case.stage(db)
        case.compose().import_into(
            db, build=case.context(), vocabulary=case.vocabulary, published=()
        )
        product(db, known.printing_id, "2026-01-01")
        with db.transaction():
            for row in db.rows("face_wording"):
                db.delete(
                    "face_wording",
                    {key: row.values[key] for key in ("face_id", "region")},
                )
            populate_wording_basis(db, case.plan)
        assessment = next(
            row.values
            for row in db.rows("face_wording")
            if row.values["region"] == "jp"
        )
        assert assessment["classification"] == "same_effect"
        assert assessment["basis_printing_id"] == known.printing_id
