"""Corrected projections require verified evidence and cannot alter sealed originals."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.provenance import BuildContext
from sve_carddb.domains.translations.corrected_sources import PARSER, Corrections
from sve_carddb.domains.translations.sources import Sources, pointer

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from ...support.shared_case_fixtures import CorrectionCaseTemplate


def test_explicit_recipe_replays_only_its_verified_effect_and_keeps_original(
    tmp_path: Path, default_correction_case: CorrectionCaseTemplate
) -> None:
    case = default_correction_case.copy(tmp_path).texts
    proof = Corrections(case.plan)
    assert case.plan.corrections is not None
    application = case.plan.corrections[0]
    observation = application.observation
    source = observation.card.source
    assert source.archive is not None
    sources = Sources(
        {"test-store": case.store}, case.root, case.context(), corrections=proof
    )
    key = source.archive.batch_id, source.id
    locator = f"/faces/{observation.source_index}/text"
    original: JsonValue = {"faces": [{"text": application.data.expected_raw_value}]}
    projected = proof.project(*key, original)
    assert pointer(original, locator) == application.data.expected_raw_value
    assert pointer(projected, locator) == application.data.corrected_value
    assert sources.stage(case.context()).corrections is proof
    proof.verify_scope(
        *key, observation.source_index, observation.card_id, observation.face_id
    )
    with pytest.raises(ValueError, match="another owner"):
        proof.verify_scope(
            *key, observation.source_index, "another-card", observation.face_id
        )
    with pytest.raises(ValueError, match="exact original"):
        proof.project(*key, {"faces": [{"text": "Wrong original"}]})
    with pytest.raises(ValueError, match="face is absent"):
        proof.project(*key, {"faces": []})
    with pytest.raises(ValueError, match="no applied"):
        proof.project(key[0], "another-version", original)
    with pytest.raises(ValueError, match="pinned correction plan"):
        Sources({"test-store": case.store}, case.root, case.context()).projection(
            *key, PARSER
        )


def test_missing_or_altered_plan_is_rejected_before_projection(
    tmp_path: Path, default_correction_case: CorrectionCaseTemplate
) -> None:
    case = default_correction_case.copy(tmp_path).texts
    with pytest.raises(ValueError, match="pinned correction plan"):
        Corrections(replace(case.plan, corrections=None))
    with pytest.raises(ValueError, match="Correction"):
        Corrections(replace(case.plan, corrections=()))
    proof = Corrections(case.plan)
    with pytest.raises(ValueError, match="pin its correction plan"):
        proof.verify_context(
            BuildContext.from_inputs(
                case.context().program_revision,
                {"text_observations": {"corrections_hash": "sha256:" + "0" * 64}},
            )
        )
