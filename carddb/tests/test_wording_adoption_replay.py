"""Independent immutable replay, predecessor and atomic import counterexamples."""

import dataclasses
import sqlite3
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.snapshot.values import canonical, object_value, parse
from sve_carddb.text_observations.importer import populate_text_observations
from sve_carddb.text_observations.models import candidate_revision_id
from sve_carddb.text_observations.wording import wording_views
from sve_carddb.wording_adoptions.importer import import_adoptions
from sve_carddb.wording_adoptions.models import Mechanical
from sve_carddb.wording_adoptions.reconstruction import Reconstructor
from sve_carddb.wording_adoptions.replay import replay_adoptions
from sve_carddb.wording_adoptions.semantics import bundle_ready

from .wording_adoption_fixtures import make_adoption_case

if TYPE_CHECKING:
    from sve_carddb.wording_adoptions.loader import AdoptionSnapshot
    from sve_carddb.wording_adoptions.models import AdoptionRecord

    from .wording_adoption_fixtures import AdoptionCase


@pytest.fixture(scope="module", params=[False, True], ids=["policy", "human"])
def adoption_case(
    tmp_path_factory: pytest.TempPathFactory, request: pytest.FixtureRequest
) -> AdoptionCase:
    return make_adoption_case(
        tmp_path_factory.mktemp("wording-adoption"), human=request.param
    )


@pytest.fixture(scope="module")
def reconstruction(adoption_case: AdoptionCase) -> Reconstructor:
    return Reconstructor(adoption_case.root, {"wording-store": adoption_case.store})


def test_full_raw_history_and_mechanical_predecessor_are_reproduced(
    adoption_case: AdoptionCase, reconstruction: Reconstructor
) -> None:
    case = adoption_case
    replayed = replay_adoptions(case.snapshot, reconstruction)
    assert len(replayed) == 1
    adoption = replayed[0]
    assert len(adoption.scope.observations) == 3
    assert adoption.predecessor_scope is not None
    assert len(adoption.predecessor_scope.observations) == 2
    assert adoption.previous is not None
    assert adoption.selected.content.effect is not None
    assert adoption.basis == (
        "reviewed_override"
        if adoption.record.data.review.mode == "human"
        else "latest_adopted_wording"
    )


@pytest.mark.parametrize(
    "change",
    [
        "dependency",
        "program",
        "registry",
        "python",
        "unicode",
        "parser",
        "projection",
        "cutoff",
        "region",
        "unknown-capability",
    ],
)
def test_each_full_context_pin_is_required(
    adoption_case: AdoptionCase, reconstruction: Reconstructor, change: str
) -> None:
    review = adoption_case.review
    context = review.context
    if change == "dependency":
        context = context.model_copy(update={"dependencies": context.dependencies[:-1]})
    elif change == "program":
        context = context.model_copy(update={"program_revision": "0" * 40})
    else:
        config = object_value(parse(context.configuration.encode()))
        if change == "registry":
            config["registry"] = object_value(config["registry"]) | {
                "authored_revision": "0" * 40
            }
        else:
            key, value = (
                ("python_version", "0.0")
                if change == "python"
                else ("unicode_version", "0.0")
                if change == "unicode"
                else ("parser", "not-reviewed")
                if change == "parser"
                else ("projection", "correction-before-absent")
                if change == "projection"
                else ("errata_as_of", "not-a-date")
                if change == "cutoff"
                else ("regions", [])
                if change == "region"
                else ("errata", "pretend-complete")
            )
            config[key] = value if isinstance(value, str) else list[JsonValue](value)
        context = context.model_copy(
            update={"configuration": canonical(config).decode()}
        )
    changed = review.model_copy(update={"context": context})
    with pytest.raises(
        ValueError,
        match=r"dependency|Pinned|pin|replayed|Invalid|revision|Literal|date|regions|input",
    ):
        reconstruction.scope(changed, adoption_case.face_id, "jp")


def snapshot_record(case: AdoptionCase, record: AdoptionRecord) -> AdoptionSnapshot:
    return dataclasses.replace(case.snapshot, records={record.record_key: record})


@pytest.mark.parametrize(
    "change", ["missing", "raw-face", "corrected-content", "parser", "presence"]
)
def test_corrected_inventory_is_compared_independently(
    adoption_case: AdoptionCase, reconstruction: Reconstructor, change: str
) -> None:
    record = next(iter(adoption_case.snapshot.records.values()))
    observations = record.data.observations
    if change == "missing":
        observations = observations[:-1]
    else:
        old = observations[0]
        fields: dict[str, dict[str, object]] = {
            "raw-face": {"raw_face_hash": "sha256:" + "0" * 64},
            "corrected-content": {"content_hash": "sha256:" + "0" * 64},
            "parser": {"parser_version": "different-parser"},
            "presence": {"effect_presence": observations[-1].effect_presence},
        }
        changed = old.model_copy(update=fields[change])
        if change == "presence" and changed == old:
            changed = old.model_copy(update={"source_index": 99})
        observations = (changed, *observations[1:])
    record = record.model_copy(
        update={"data": record.data.model_copy(update={"observations": observations})}
    )
    with pytest.raises(ValueError, match="complete corrected inventory"):
        replay_adoptions(snapshot_record(adoption_case, record), reconstruction)


@pytest.mark.parametrize(
    "change",
    [
        "root-missing",
        "root-selection",
        "root-evidence",
        "previous-order",
        "same-content",
    ],
)
def test_predecessor_is_not_a_hash_or_an_implicit_order_answer(
    adoption_case: AdoptionCase, reconstruction: Reconstructor, change: str
) -> None:
    record = next(iter(adoption_case.snapshot.records.values()))
    previous = record.data.previous
    assert isinstance(previous, Mechanical)
    if change == "root-missing":
        previous = previous.model_copy(
            update={"observations": previous.observations[:-1]}
        )
        record = record.model_copy(
            update={"data": record.data.model_copy(update={"previous": previous})}
        )
    elif change == "root-selection":
        key = next(
            o.observation_key
            for o in previous.observations
            if o.observation_key != previous.selected_observation_key
        )
        record = record.model_copy(
            update={
                "data": record.data.model_copy(
                    update={
                        "previous": previous.model_copy(
                            update={"selected_observation_key": key}
                        )
                    }
                )
            }
        )
    elif change == "root-evidence":
        batch = previous.review_context.source_batches[0].batch_id
        record = record.model_copy(
            update={
                "evidence": tuple(e for e in record.evidence if e.batch_id != batch)
            }
        )
    elif change == "previous-order":
        record = record.model_copy(
            update={"data": record.data.model_copy(update={"previous_order": None})}
        )
    else:
        if record.data.review.mode != "human":
            return
        assert record.data.previous_order is not None
        order = record.data.previous_order.model_copy(
            update={
                "basis": "same_content",
                "evidence_indexes": (),
                "review_receipt": None,
            }
        )
        record = record.model_copy(
            update={"data": record.data.model_copy(update={"previous_order": order})}
        )
    with pytest.raises(ValueError, match=r"Mechanical|Predecessor|Same-content|omits"):
        replay_adoptions(snapshot_record(adoption_case, record), reconstruction)


@pytest.mark.parametrize("newer", [False, True])
def test_atomic_import_preserves_physical_history_current_and_unchecked_versions(
    adoption_case: AdoptionCase, newer: bool
) -> None:
    case = adoption_case
    with create_database(compile_build(("t0", "semantics"))) as db:
        with db.transaction():
            case.base.stage(db)
        with db.transaction():
            populate_text_observations(
                db,
                case.base.plan,
                build=case.base.context(),
                vocabulary=case.base.vocabulary,
                published=(),
            )
        inputs = import_adoptions(db, **case.inputs(newer=newer))
        selected = case.replayed[0].selected
        assert db.rows("face_current")[0].values[
            "revision_id"
        ] == candidate_revision_id(selected)
        assert (
            db.rows("face_current")[0].values["decision_id"]
            == case.replayed[0].decision.id
        )
        assert len(db.rows("face_semantics")) == 1
        assert len(db.rows("printing_face_observation")) == (6 if newer else 5)
        assert all(
            r.values["parser_version"] is None
            for r in db.rows("source_record")
            if r.values["kind"] != "authored"
        )
        assert len(inputs.uses) > len(case.scope.uses)
        assert all(
            r.values["automatic"] is False for r in db.rows("card_engine_support")
        )
        views = wording_views(db, case.base.plan)
        if newer:
            assert views[case.face_id][0].display.basis == "current"
            assert views[case.face_id][0].display.revision_id == candidate_revision_id(
                selected
            )
        else:
            assert case.face_id not in views
        assert bundle_ready(db, candidate_revision_id(selected))
        db.verify()


@pytest.mark.parametrize(
    "change", ["dependency", "program", "configuration", "capability", "vocabulary"]
)
def test_any_failed_input_rolls_back_the_entire_adoption(
    adoption_case: AdoptionCase, change: str
) -> None:
    case = adoption_case
    inputs = case.inputs()
    if change == "dependency":
        inputs["build"] = inputs["build"].model_copy(
            update={"dependencies": inputs["build"].dependencies[:-1]}
        )
    elif change == "program":
        inputs["build"] = inputs["build"].model_copy(
            update={"program_revision": "0" * 40}
        )
    elif change == "configuration":
        inputs["build"] = inputs["build"].model_copy(
            update={"configuration": canonical({}).decode()}
        )
    elif change == "vocabulary":
        inputs["vocabulary"] = inputs["vocabulary"].model_copy(update={"bindings": ()})
    capabilities = ("t0",) if change == "capability" else ("t0", "semantics")
    with create_database(compile_build(capabilities)) as db:
        with db.transaction():
            case.base.stage(db)
        before = {
            table.name: db.rows(table.name)
            for table in compile_build(capabilities).tables
        }
        with pytest.raises((ValueError, sqlite3.IntegrityError)):
            import_adoptions(db, **inputs)
        assert {
            table.name: db.rows(table.name)
            for table in compile_build(capabilities).tables
        } == before
