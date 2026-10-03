"""Raw and corrected revisions independently retain their own frozen name proof."""

from typing import TYPE_CHECKING

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.digital_name_policies.owners import publication_owners
from sve_carddb.text_observations import import_text_observations
from sve_carddb.text_observations.models import candidate_revision_id

from .source_correction_fixtures import make_correction_case
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.review import Inputs


def test_raw_history_remains_eligible_after_effect_correction(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_correction_case(tmp_path, inputs).texts
    assert case.plan.corrections is not None
    raw = case.plan.corrections[0].observation
    corrected = next(
        item for item in case.plan.candidates() if item.printing_id == raw.printing_id
    )
    assert candidate_revision_id(raw) != candidate_revision_id(corrected)
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
        owners = dict(publication_owners(db, case.plan))
        revisions = {
            owner.identifier: evidence
            for owner, evidence in owners.items()
            if owner.kind == "face_revision"
        }
        for item in (raw, corrected):
            evidence = revisions[candidate_revision_id(item)]
            assert evidence.name_ref is not None
            assert evidence.name_ref.source_version_id == item.card.source.id
            assert evidence.name_ref.locator == f"/faces/{item.source_index}/name"
        assert (
            revisions[candidate_revision_id(raw)].name_ref
            == revisions[candidate_revision_id(corrected)].name_ref
        )
