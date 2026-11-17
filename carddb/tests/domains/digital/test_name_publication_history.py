"""Raw and corrected revisions independently retain their own frozen name proof."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.domains.digital.name_policies.owners import publication_owners
from sve_carddb.domains.registry.build import build
from sve_carddb.domains.registry.storage import load, plan_files, write_files
from sve_carddb.domains.text_observations.models import candidate_revision_id

from ...support.source_correction_fixtures import make_correction_case
from ..registry.test_registry import card, card_of
from ..registry.test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.registry.review import Inputs


def test_raw_history_remains_eligible_after_effect_correction(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_correction_case(tmp_path, inputs).texts
    raw = case.plan.corrections[0].observation
    corrected = next(
        item for item in case.plan.candidates() if item.printing_id == raw.printing_id
    )
    assert candidate_revision_id(raw) != candidate_revision_id(corrected)
    with create_database(compile_build(("en", "related", "correction"))) as db:
        with db.transaction():
            case.stage(db)
        with db.transaction():
            case.compose(case.plan.identity).populate(
                db,
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


def test_corrected_name_is_proven_by_its_faces_official_printing(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.jp["BP02-071"].faces[0].name = "名前（EVOLVE）"
    inputs.jp["PR-001"] = card("PR-001", "名前")
    authored = tmp_path / "authored"
    write_files(plan_files(authored, build(inputs, {})))
    first = load(authored)[1]
    keep = card_of(list(first.values()), "jp", "PR-001")
    inputs.decisions.same_cards = {keep: ["jp:BP02-071"]}
    case = make_correction_case(
        tmp_path, inputs, field="name", corrected="名前", fold=True
    ).texts
    corrected = next(item for item in case.plan.candidates() if item.correction_keys)
    official = next(item for item in case.plan.candidates() if item.card_no == "PR-001")
    assert (corrected.card_id, corrected.face_id) == (
        official.card_id,
        official.face_id,
    )
    with create_database(compile_build(("en", "related", "correction"))) as db:
        with db.transaction():
            case.stage(db)
        with db.transaction():
            case.compose(case.plan.identity).populate(
                db,
                build=case.context(),
                vocabulary=case.vocabulary,
                published=(),
            )
        owners = {
            owner.identifier: evidence
            for owner, evidence in publication_owners(db, case.plan)
            if owner.kind == "face_revision"
        }
    evidence = owners[candidate_revision_id(corrected)]
    assert evidence.state == "known"
    assert evidence.printing_id == official.printing_id
    assert evidence.name_ref is not None
    assert evidence.name_ref.source_version_id == official.card.source.id


def test_corrected_name_without_official_text_stays_unproven(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_correction_case(tmp_path, inputs, field="name").texts
    with create_database(compile_build(("en", "related", "correction"))) as db:
        with db.transaction():
            case.stage(db)
        with db.transaction():
            case.compose(case.plan.identity).populate(
                db,
                build=case.context(),
                vocabulary=case.vocabulary,
                published=(),
            )
        with pytest.raises(ValueError, match="verified publication source"):
            publication_owners(db, case.plan)
