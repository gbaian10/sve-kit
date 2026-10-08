"""Independent projection boundaries found by the first wording review."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json, create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.domains.text_observations import import_text_observations
from sve_carddb.domains.text_observations.wording import (
    ObservedText,
    WordingCandidate,
    _candidate_revisions,
    _canonical_observations,
    _display,
    printing_observed_texts,
    wording_views,
)

from .build_db_fixtures import rows
from .source_correction_fixtures import make_correction_case
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture
from .test_wording_pending import product
from .text_observation_fixtures import make_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.registry.review import Inputs


def test_unresolved_correction_never_selects_latest_known_display(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_correction_case(tmp_path, inputs, state="needs_review").texts
    group = next(g for g in case.plan.groups if g.region == "jp")
    assert "source_correction_pending" in group.reasons
    with create_database(compile_build(("en", "related", "correction"))) as db:
        with db.transaction():
            case.stage(db)
        import_text_observations(
            db,
            case.plan,
            build=case.context(),
            vocabulary=case.vocabulary,
            published=(),
        )
        for item in group.observations:
            product(
                db,
                item.printing_id,
                "2026-01-01" if item.card_no == "PR-001" else "2020-01-01",
            )
        projected = next(
            v for v in wording_views(db, case.plan)[group.face_id] if v.region == "jp"
        )
        assert all(c.revision_id is not None for c in projected.candidates)
        assert projected.display.basis == "candidates"
        assert projected.display.revision_id is None
        assert all(
            o.state == "available"
            for entries in printing_observed_texts(db, case.plan).values()
            for o in entries
        )


def test_display_checks_only_the_latest_day_for_unresolved_corrections() -> None:
    candidates = (
        WordingCandidate(printing_id="p:old", revision_id="r:old"),
        WordingCandidate(printing_id="p:new", revision_id="r:new"),
    )
    dates: dict[str, str | None] = {"p:old": "2020-01-01", "p:new": "2026-01-01"}
    content = {"r:old": "synthetic-old", "r:new": "synthetic-new"}
    assert (
        _display(None, candidates, dates, frozenset({"p:old"}), content).revision_id
        == "r:new"
    )
    assert (
        _display(None, candidates, dates, frozenset({"p:new"}), content).revision_id
        is None
    )


def test_candidate_positions_ignore_printings_outside_the_public_parent_scope(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    with create_database(compile_build()) as db:
        assert case.plan.candidates()
        assert _candidate_revisions(db, case.plan, {}) == {}


@pytest.mark.parametrize("boundary", ["face", "region"])
def test_candidate_revision_must_belong_to_its_face_and_region(
    tmp_path: Path, inputs: Inputs, boundary: str
) -> None:
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    with create_database(compile_build(("en",))) as db:
        with db.transaction():
            case.stage(db)
        import_text_observations(
            db,
            case.plan,
            build=case.context(),
            vocabulary=case.vocabulary,
            published=(),
        )
        target = db.rows("face_revision")[0].values
        with db.transaction():
            if boundary == "face":
                for row in db.rows("printing_face_observation"):
                    if row.values["revision_id"] == target["id"]:
                        db.delete(
                            "printing_face_observation",
                            {
                                key: row.values[key]
                                for key in ("printing_id", "face_id", "source_id")
                            },
                        )
                db.insert(
                    "face",
                    rows()["face"]
                    | {
                        "id": "synthetic-other-face",
                        "card_id": case.plan.groups[0].observations[0].card_id,
                        "ordinal": 1,
                        "side": "back",
                    },
                )
            db.update(
                "face_revision",
                {"id": target["id"]},
                {"face_id": "synthetic-other-face"}
                if boundary == "face"
                else {"region": "en"},
            )
        with pytest.raises(
            ValueError, match=r"candidate revision.*another face/region"
        ):
            wording_views(db, case.plan)


@pytest.mark.parametrize("distinct", ["revision", "state", "source"])
def test_observations_deduplicate_only_the_complete_public_triple(
    distinct: str,
) -> None:
    original = ObservedText(
        revision_id="r:a", state="available", source_url="https://example.invalid/a"
    )
    changed = ObservedText(
        revision_id="r:b" if distinct == "revision" else "r:a",
        state="correction_conflict" if distinct == "state" else "available",
        source_url="https://example.invalid/b"
        if distinct == "source"
        else original.source_url,
    )
    result = _canonical_observations([original, original, changed])
    assert len(result) == 2
    assert set(result) == {original, changed}


def test_importer_marks_only_the_region_without_a_current(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_case(tmp_path / "authored", inputs)
    assert any(g.current() is not None and g.region == "en" for g in case.plan.groups)
    card = case.plan.groups[0].observations[0].card_id
    with create_database(compile_build(("en", "related"))) as db:
        with db.transaction():
            case.stage(db)
            for region in ("jp", "en"):
                db.insert(
                    "card_engine_support",
                    rows()["card_engine_support"]
                    | {
                        "card_id": card,
                        "region": region,
                        "reason_codes": Json(["existing_manual_reason"]),
                    },
                )
        import_text_observations(
            db,
            case.plan,
            build=case.context(),
            vocabulary=case.vocabulary,
            published=(),
        )
        support = {
            row.values["region"]: row.values for row in db.rows("card_engine_support")
        }
        assert support["jp"]["reason_codes"] == Json(
            ["existing_manual_reason", "wording_pending"]
        )
        assert support["en"]["reason_codes"] == Json(["existing_manual_reason"])
        assert support["en"]["automatic"] is False
