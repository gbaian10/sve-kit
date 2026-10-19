"""Semantic selection, chronology and stale decisions use synthetic source text."""

from dataclasses import replace
from typing import TYPE_CHECKING, Literal

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.domains.text_observations.equivalence import (
    WordingDecision,
    comparison_hash,
    comparison_text,
    load_decisions,
)
from sve_carddb.domains.text_observations.models import candidate_revision_id
from sve_carddb.domains.text_observations.wording import populate_wording_basis

from ...support.source_correction_fixtures import make_correction_case
from ...support.text_observation_fixtures import make_case
from ..registry.test_registry import card
from ..registry.test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture
from .test_wording_pending import product

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.regions import Region
    from sve_carddb.domains.registry.review import Inputs


@pytest.mark.parametrize("region", ["jp", "en"])
@pytest.mark.parametrize(
    "mode",
    ["main_reminder", "token_reminder", "supplement", "wording", "tie", "undated"],
)
def test_current_wording_preferences_precede_release_dates(
    tmp_path: Path,
    inputs: Inputs,
    region: Region,
    mode: str,
) -> None:
    if region == "en":
        del inputs.en["GF01-001EN"]
        del inputs.mapping.targets["GF01-001EN"]
        inputs.mapping.reskins = {}
        inputs.en["PR-002EN"] = card("PR-002EN", "Name", english=True)
        inputs.mapping.targets["PR-002EN"] = "BP02-071"
        inputs.mapping.original_art.add("PR-002EN")
        inputs.decisions.art_groups = [["BP02-070EN", "PR-002EN"]]
    cards = inputs.jp if region == "jp" else inputs.en
    older, newer = tuple(cards.values())
    newer.faces[0].name = older.faces[0].name
    plain = "【必殺】" if region == "jp" else "Bane."
    reminder = (
        "【必殺】（【必殺】を持つ合成ユニットの説明）"
        if region == "jp"
        else "Bane. (After this follower fights, synthetic reminder.)"
    )
    older.faces[0].text = plain if mode == "main_reminder" else "Create a token."
    newer.faces[0].text = (
        reminder
        if mode == "main_reminder"
        else "Create two tokens."
        if mode in {"wording", "tie", "undated"}
        else "Create a token."
    )
    if mode in {"token_reminder", "supplement"}:
        older.faces[0].sections = [plain]
        newer.faces[0].sections = [reminder] if mode == "token_reminder" else []
    case = make_case(tmp_path / "authored", inputs, regions=(region,))
    group = case.plan.groups[0]
    with create_database(compile_build(("t0", "en", "related"))) as db:
        with db.transaction():
            case.stage(db)
        case.compose().import_into(
            db, build=case.context(), vocabulary=case.vocabulary, published=()
        )
        for item in group.observations:
            product(
                db,
                item.printing_id,
                None
                if mode == "undated"
                else "2020-01-01"
                if mode == "tie" or item.card_no == older.number
                else "2026-01-01",
                "unknown" if mode == "undated" else "day",
            )
        with db.transaction():
            for row in db.rows("face_wording"):
                db.delete(
                    "face_wording",
                    {key: row.values[key] for key in ("face_id", "region")},
                )
            populate_wording_basis(db, case.plan)
        if mode == "undated":
            assert not db.rows("face_current")
            assert db.rows("face_wording")[0].values["reason"] == "no_known_release_day"
            return
        expected_no = (
            newer.number
            if mode == "wording"
            else min(cards)
            if mode == "tie"
            else older.number
        )
        expected = next(
            item for item in group.observations if item.card_no == expected_no
        )
        assert db.rows("face_current")[0].values[
            "revision_id"
        ] == candidate_revision_id(expected)


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
        "ai_undated_pending",
        "errata",
        "manual_reject_auto",
        "manual_same_day",
    ],
)
def test_current_selection_keeps_classification_and_breaks_release_ties(
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
        else "pending_review"
        if mode == "ai_undated_pending"
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
            if (
                mode in {"undated", "ai_undated_pending", "manual_reject_auto"}
                and i == 1
            ):
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
        assert len(db.rows("face_current")) == 1
        expected = group.observations[
            0
            if mode
            in {
                "manual",
                "undated",
                "ai_undated_pending",
                "manual_same_day",
                "same_day",
                "manual_reject_auto",
            }
            else 1
        ]
        if mode == "same_day":
            expected = min(group.observations, key=lambda item: item.card_no)
        assert db.rows("face_current")[0].values[
            "revision_id"
        ] == candidate_revision_id(expected)
        if mode in {"errata", "manual_reject_auto"}:
            assert assessment["classification"] == "errata"
            assert assessment["basis_printing_id"] is None
        else:
            assert assessment["basis_printing_id"] == expected.printing_id
            assert assessment["classification"] == "same_effect"
            if mode == "ai_undated_pending":
                assert assessment["method"] == "mechanical"
                assert assessment["reason"] == "undated_difference_excluded"


@pytest.mark.parametrize(
    ("method", "classification", "error"),
    [
        ("ai", "same_effect", "Only manual decisions"),
        ("manual", "errata", "Only equivalent wording"),
    ],
)
def test_only_manual_equivalence_may_choose_a_basis(
    method: Literal["ai", "manual"],
    classification: Literal["same_effect", "errata"],
    error: str,
) -> None:
    with pytest.raises(ValueError, match=error):
        WordingDecision(
            face_id="f:synthetic",
            region="jp",
            comparison_hash="sha256:" + "0" * 64,
            classification=classification,
            method=method,
            reason="Synthetic review",
            basis_printing_id="p:synthetic",
        )


@pytest.mark.parametrize(
    "mode", ["current_preserved", "manual_overrides", "orphan", "unknown_basis"]
)
def test_adopted_current_wins_unless_a_manual_basis_is_locatable(
    tmp_path: Path, inputs: Inputs, mode: str
) -> None:
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    group = case.plan.groups[0]
    older, newer = group.observations[0], group.observations[1]
    decision = WordingDecision(
        face_id="f:synthetic" if mode == "orphan" else group.face_id,
        region="jp",
        comparison_hash=comparison_hash(tuple(i.content for i in group.observations)),
        classification="same_effect",
        method="ai" if mode == "current_preserved" else "manual",
        reason="Synthetic reviewed equivalence",
        basis_printing_id=None
        if mode == "current_preserved"
        else "p:synthetic"
        if mode == "unknown_basis"
        else older.printing_id,
    )
    with create_database(compile_build()) as db:
        with db.transaction():
            case.stage(db)
        case.compose().import_into(
            db, build=case.context(), vocabulary=case.vocabulary, published=()
        )
        for item in group.observations:
            product(
                db, item.printing_id, "2020-01-01" if item is older else "2026-01-01"
            )
        plan = replace(case.plan, wording_decisions=(decision,))
        with db.transaction():
            for row in db.rows("face_wording"):
                db.delete(
                    "face_wording",
                    {key: row.values[key] for key in ("face_id", "region")},
                )
            db.insert(
                "face_current",
                {
                    "face_id": group.face_id,
                    "region": "jp",
                    "revision_id": candidate_revision_id(newer),
                    "basis": "latest_observed_no_errata",
                    "decision_id": None,
                },
            )
            if mode in {"orphan", "unknown_basis"}:
                with pytest.raises(
                    ValueError,
                    match="no pending face/region"
                    if mode == "orphan"
                    else "no unique observation",
                ):
                    populate_wording_basis(db, plan)
                return
            populate_wording_basis(db, plan)
        assessment = db.rows("face_wording")[0].values
        (current,) = (row.values for row in db.rows("face_current"))
        if mode == "current_preserved":
            assert assessment["classification"] == "unresolved"
            assert assessment["reason"] == "adopted_current_preserved"
            assert assessment["basis_printing_id"] is None
            assert current["revision_id"] == candidate_revision_id(newer)
            assert current["basis"] == "latest_observed_no_errata"
        else:
            assert assessment["classification"] == "same_effect"
            assert assessment["basis_printing_id"] == older.printing_id
            assert assessment["basis_released_on"] == "2020-01-01"
            assert current["revision_id"] == candidate_revision_id(older)
            assert current["basis"] == "latest_known_release"


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


@pytest.mark.parametrize(
    ("mode", "empty_day", "full_day"),
    [
        ("undated_full", "2026-01-01", None),
        ("ai_pending", "2026-01-01", None),
        ("older_full", "2026-01-01", "2020-01-01"),
        ("manual_empty", "2026-01-01", "2020-01-01"),
        ("existing_empty_current", "2026-01-01", None),
        ("all_empty", "2026-01-01", None),
        ("nonempty_dates", "2020-01-01", "2026-01-01"),
    ],
)
def test_shared_basis_uses_rules_text_instead_of_an_empty_printing(
    tmp_path: Path,
    inputs: Inputs,
    mode: str,
    empty_day: str,
    full_day: str | None,
) -> None:
    inputs.jp["BP02-071"].faces[0].text = (
        "Draw a card." if mode == "nonempty_dates" else ""
    )
    inputs.jp["PR-001"].faces[0].text = "" if mode == "all_empty" else "Draw a card;"
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    group = case.plan.groups[0]
    empty = next(o for o in group.observations if o.card_no == "BP02-071")
    full = next(o for o in group.observations if o.card_no == "PR-001")
    plan = (
        replace(
            case.plan,
            wording_decisions=(
                WordingDecision(
                    face_id=group.face_id,
                    region="jp",
                    comparison_hash=comparison_hash(
                        tuple(o.content for o in group.observations)
                    ),
                    classification="same_effect"
                    if mode == "manual_empty"
                    else "pending_review",
                    method="manual" if mode == "manual_empty" else "ai",
                    basis_printing_id=empty.printing_id
                    if mode == "manual_empty"
                    else None,
                    reason="Synthetic blank and full-text difference",
                ),
            ),
        )
        if mode in {"ai_pending", "manual_empty"}
        else case.plan
    )
    with create_database(compile_build()) as db:
        with db.transaction():
            case.stage(db)
        case.compose().import_into(
            db, build=case.context(), vocabulary=case.vocabulary, published=()
        )
        product(db, empty.printing_id, empty_day)
        product(
            db, full.printing_id, full_day, "unknown" if full_day is None else "day"
        )
        with db.transaction():
            for row in db.rows("face_wording"):
                db.delete(
                    "face_wording",
                    {key: row.values[key] for key in ("face_id", "region")},
                )
            if mode != "all_empty":
                # Product evidence is added after the fixture's initial import.
                for row in db.rows("face_current"):
                    db.delete(
                        "face_current",
                        {key: row.values[key] for key in ("face_id", "region")},
                    )
            if mode == "existing_empty_current":
                db.insert(
                    "face_current",
                    {
                        "face_id": group.face_id,
                        "region": "jp",
                        "revision_id": candidate_revision_id(empty),
                        "basis": "latest_observed_no_errata",
                        "decision_id": None,
                    },
                )
            if mode == "manual_empty":
                with pytest.raises(ValueError, match="no unique observation"):
                    populate_wording_basis(db, plan)
                return
            populate_wording_basis(db, plan)
        (current,) = (row.values for row in db.rows("face_current"))
        if mode == "all_empty":
            revision = next(
                row.values
                for row in db.rows("face_revision")
                if row.values["id"] == current["revision_id"]
            )
            assert any(
                row.values["id"] == revision["effect_unit_id"]
                and not row.values["text"]
                for row in db.rows("text_unit")
            )
        else:
            assert current["revision_id"] == candidate_revision_id(full)
            assessment = db.rows("face_wording")[0].values
            assert assessment["classification"] == "same_effect"
            assert assessment["basis_printing_id"] == full.printing_id
            assert assessment["basis_released_on"] == full_day
            assert current["basis"] == (
                "latest_observed_no_errata"
                if full_day is None
                else "latest_known_release"
            )


@pytest.mark.parametrize("region", ["jp", "en"])
@pytest.mark.parametrize("correct_source", [False, True])
def test_corrected_missing_rule_is_not_reported_as_a_reminder(
    tmp_path: Path, inputs: Inputs, region: Region, correct_source: bool
) -> None:
    if region == "en":
        del inputs.en["GF01-001EN"]
        del inputs.mapping.targets["GF01-001EN"]
        inputs.mapping.reskins = {}
        inputs.en["PR-002EN"] = card("PR-002EN", "Name", english=True)
        inputs.mapping.targets["PR-002EN"] = "BP02-071"
        inputs.mapping.original_art.add("PR-002EN")
        inputs.decisions.art_groups = [["BP02-070EN", "PR-002EN"]]
    cards = inputs.jp if region == "jp" else inputs.en
    incomplete, complete = tuple(cards.values())
    complete.faces[0].name = incomplete.faces[0].name
    incomplete.faces[0].text = "【疾走】" if region == "jp" else "Storm."
    complete.faces[0].text = (
        incomplete.faces[0].text + "\nResolve a synthetic end rule."
    )
    case = (
        make_correction_case(
            tmp_path, inputs, region=region, corrected=complete.faces[0].text[:-1] + ";"
        ).texts
        if correct_source
        else make_case(tmp_path / "authored", inputs, regions=(region,))
    )
    group = next(group for group in case.plan.groups if group.region == region)
    with create_database(compile_build(("en", "related", "correction"))) as db:
        with db.transaction():
            case.stage(db)
        case.compose().import_into(
            db, build=case.context(), vocabulary=case.vocabulary, published=()
        )
        for item in group.observations:
            product(db, item.printing_id, "2026-01-01")
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
            if row.values["region"] == region
        )
        assert assessment["classification"] == (
            "same_effect" if correct_source else "pending_review"
        )
        assert assessment["method"] == ("mechanical" if correct_source else "none")
        assert assessment["reason"] == (
            "source_correction" if correct_source else "semantic_review_required"
        )
