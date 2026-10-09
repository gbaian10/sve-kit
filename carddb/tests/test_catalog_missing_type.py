"""Missing EN type classification retains exact raw and verified correction provenance."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json, create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import array, object_value
from sve_carddb.domains.source_corrections.projection import correction_references
from sve_carddb.domains.text_observations import Binding, Vocabulary
from sve_carddb.domains.text_observations.importer import revision_id
from sve_carddb.domains.text_observations.type_binding import type_binding

from .source_correction_fixtures import CorrectionCase, make_correction_case
from .test_registry import make_inputs

if TYPE_CHECKING:
    from sve_carddb.domains.source_corrections.plan import Application


@pytest.fixture(scope="module")
def correction(tmp_path_factory: pytest.TempPathFactory) -> CorrectionCase:
    inputs = make_inputs()
    inputs.mapping.targets["BP02-070EN"] = None
    result = make_correction_case(
        tmp_path_factory.mktemp("missing-en-type"),
        inputs,
        region="en",
        field="card_type",
        raw_type="-",
        corrected="Evolution Point",
    )
    result.texts.vocabulary = Vocabulary(
        bindings=(
            *result.texts.vocabulary.bindings,
            Binding(region="en", kind="type", raw="Evolution Point", code="ep"),
        )
    )
    return result


def application(correction: CorrectionCase) -> Application:
    assert correction.texts.plan.corrections is not None
    return correction.texts.plan.corrections[0]


def test_missing_type_uses_exact_confirmed_correction_and_preserves_both_revisions(
    correction: CorrectionCase,
) -> None:
    case = correction.texts
    applied = application(correction)
    raw = applied.observation
    candidate = next(
        item for item in case.plan.candidates() if item.printing_id == raw.printing_id
    )
    assert raw.content.type_raw == "-"
    assert candidate.content.type_raw == "Evolution Point"
    assert not [
        binding
        for binding in case.vocabulary.bindings
        if binding.kind == "type" and binding.raw == "-"
    ]
    with create_database(compile_build(("en", "related", "correction"))) as db:
        with db.transaction():
            case.stage(db)
        record = case.compose(case.plan.identity).import_into(
            db,
            build=case.context(),
            vocabulary=case.vocabulary,
            published=(),
        )
        revisions = {row.values["id"]: row.values for row in db.rows("face_revision")}
        initial, corrected = (
            revisions[revision_id(raw)],
            revisions[revision_id(candidate)],
        )
        assert initial["type_code"] == "ep"
        assert initial["decision_id"] is None
        assert initial["source_id"] == raw.card.source.id
        assert initial["change_kind"] == "initial"
        assert initial["supersedes_id"] is None
        assert corrected["type_code"] == "ep"
        assert corrected["supersedes_id"] == revision_id(raw)
        assert corrected["change_kind"] == "source_correction"
        physical = next(
            row.values
            for row in db.rows("printing_face_observation")
            if row.values["printing_id"] == raw.printing_id
        )
        assert physical["revision_id"] == revision_id(raw)
        assert db.rows("source_correction")[0].values["expected_raw_value"] == Json("-")
        assert db.rows("correction_application")[0].values[
            "face_revision_id"
        ] == revision_id(candidate)
        references = correction_references(db, case.plan, case.vocabulary)
        marker = object_value(array(references[0]["corrections"])[0])
        assert marker["corrected_from"] == "-"
        assert any(use.usage == "source_correction_evidence" for use in record.uses)


@pytest.mark.parametrize(
    "fault", ["absent", "duplicate", "pending", "conflict", "other_face", "wrong_field"]
)
def test_missing_type_never_borrows_a_nonunique_unapplied_or_unrelated_correction(
    correction: CorrectionCase, fault: str
) -> None:
    case = correction.texts
    applied = application(correction)
    applications: tuple[Application, ...] = (applied,)
    if fault == "absent":
        applications = ()
    elif fault == "duplicate":
        applications = (applied, applied)
    elif fault in {"pending", "conflict"}:
        applications = (
            replace(applied, status=None if fault == "pending" else "conflict"),
        )
    elif fault == "other_face":
        other = next(
            item
            for item in case.plan.observations
            if item.printing_id != applied.observation.printing_id
        )
        applications = (replace(applied, observation=other),)
    else:
        data = applied.data.model_copy(update={"field": "effect"})
        applications = (replace(applied, record=replace(applied.record, data=data)),)
    plan = replace(case.plan, corrections=applications)
    with pytest.raises(
        ValueError,
        match=r"^Missing raw type requires exactly one applicable source correction$",
    ):
        type_binding(plan, applied.observation, case.vocabulary)


def test_missing_type_rechecks_selected_correction_instead_of_trusting_status(
    correction: CorrectionCase,
) -> None:
    case = correction.texts
    applied = application(correction)
    forged = replace(
        applied, record=replace(applied.record, shard_path="registry/forged.yaml")
    )
    with pytest.raises(
        ValueError, match=r"^Correction record or application status mismatch$"
    ):
        type_binding(
            replace(case.plan, corrections=(forged,)),
            applied.observation,
            case.vocabulary,
        )
