"""History continuity, sealed build artifacts, late rollback and immutable graphs."""

import dataclasses
import shutil
import sqlite3
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_bundle import publish_bundle, verify_bundle
from sve_carddb.build_db import Json, create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import input_record, uses_sorted
from sve_carddb.products import product_preview_uses
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.text_observations.models import candidate_revision_id
from sve_carddb.wording_adoptions import importer
from sve_carddb.wording_adoptions.loader import load_adoptions
from sve_carddb.wording_adoptions.models import AdoptionRecord
from sve_carddb.wording_adoptions.reconstruction import Reconstructor
from sve_carddb.wording_adoptions.replay import replay_adoptions
from sve_carddb.wording_adoptions.semantics import bundle_ready

from .wording_adoption_fixtures import commit, install_adoptions, make_adoption_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import InputRecord
    from sve_carddb.text_observations.models import FaceObservation

    from .wording_adoption_fixtures import AdoptionCase


@pytest.fixture(scope="module")
def adoption_case(tmp_path_factory: pytest.TempPathFactory) -> AdoptionCase:
    return make_adoption_case(tmp_path_factory.mktemp("wording-integration"))


@pytest.mark.parametrize("change", ["none", "hash", "decision", "record", "gap"])
def test_second_adoption_replays_the_exact_previous_confirmed_receipt(
    adoption_case: AdoptionCase, tmp_path: Path, change: str
) -> None:
    case = adoption_case
    root = tmp_path / "repository"
    shutil.copytree(case.root, root)
    prior = case.replayed[0]
    record = prior.record
    number = 3 if change == "gap" else 2
    previous = {
        "kind": "adoption",
        "record_key": "missing" if change == "record" else record.record_key,
        "record_hash": digest(b"wrong")
        if change == "hash"
        else digest(canonical(record.model_dump(mode="json"))),
        "decision_id": "d:" + "0" * 64 if change == "decision" else prior.decision.id,
    }
    wire = record.model_dump(mode="json")
    wire["record_key"] = canonical(
        ["wording_adoption", case.face_id, "jp", number]
    ).decode()
    wire["data"]["adoption_no"] = number
    wire["data"]["previous"] = previous
    second = AdoptionRecord.model_validate_json(canonical(wire))
    install_adoptions(root / "authored", [second], sequence="002")
    revision = commit(root)
    if change != "none":
        with pytest.raises(ValueError, match=r"predecessor|consecutive"):
            load_adoptions(
                root / "authored",
                authored_revision=revision,
                registry=case.scope.registry,
                stores={"wording-store": case.store},
            )
        return
    snapshot = load_adoptions(
        root / "authored",
        authored_revision=revision,
        registry=case.scope.registry,
        stores={"wording-store": case.store},
    )
    reconstruction = Reconstructor(root, {"wording-store": case.store})
    replayed = replay_adoptions(snapshot, reconstruction)
    assert len(replayed) == 2
    assert replayed[1].previous == replayed[0].selected
    chained = dataclasses.replace(
        case, root=root, snapshot=snapshot, replayed=replayed, inputs_cache={}
    )
    with create_database(compile_build(("t0", "semantics"))) as db:
        with db.transaction():
            case.base.stage(db)
        importer.import_adoptions(db, **chained.inputs())
        assert len(db.rows("face_semantics")) == 1
        assert (
            db.rows("face_current")[0].values["decision_id"] == replayed[1].decision.id
        )


def test_two_clean_four_file_bundles_are_identical_and_trace_every_historical_use(
    adoption_case: AdoptionCase, tmp_path: Path
) -> None:
    case = adoption_case
    inputs = case.inputs(newer=True)
    prepared = importer.prepare_adoptions(inputs)
    stores = {"wording-store": case.store, "test-store": case.base.store}
    expected = uses_sorted(
        (
            *prepared.uses,
            *product_preview_uses(case.base.catalog, case.base.identity, stores),
        )
    )
    schema = compile_build(("t0", "semantics"))

    def populate(db: Database) -> InputRecord:
        case.base.stage(db)
        importer.populate_adoptions(db, **inputs)
        return input_record(inputs["build"], expected)

    report = importer.adoption_report(prepared)
    roots = (tmp_path / "first", tmp_path / "second")
    for root in roots:
        record = publish_bundle(
            schema, root, inputs["build"], expected, populate, report, stores=stores
        )
        assert record == verify_bundle(
            schema, root, inputs["build"], expected, stores=stores
        )
        assert {u.usage for u in record.uses} >= {
            "wording_observation",
            "effect_presence",
            "wording_evidence_closure",
        }
    for name in ("build.sqlite", "inputs.json", "report.json", "seal.json"):
        assert (roots[0] / name).read_bytes() == (roots[1] / name).read_bytes()
    assert b"Synthetic paragraph" not in (roots[0] / "report.json").read_bytes()
    with pytest.raises(ValueError, match="closure"):
        verify_bundle(schema, roots[0], inputs["build"], expected[:-1], stores=stores)


def test_failure_after_two_physical_writes_rolls_back_every_new_row(
    adoption_case: AdoptionCase, monkeypatch: pytest.MonkeyPatch
) -> None:
    case = adoption_case
    original = importer._physical
    calls = 0

    def fail(db: Database, item: FaceObservation) -> None:
        nonlocal calls
        calls += 1
        original(db, item)
        if calls == 2:
            raise ValueError("Synthetic late failure")

    monkeypatch.setattr(importer, "_physical", fail)
    schema = compile_build(("t0", "semantics"))
    with create_database(schema) as db:
        with db.transaction():
            case.base.stage(db)
        before = {t.name: db.rows(t.name) for t in schema.tables}
        with pytest.raises(ValueError, match="Synthetic late"):
            importer.import_adoptions(db, **case.inputs())
        assert calls == 2
        assert before == {t.name: db.rows(t.name) for t in schema.tables}


@pytest.mark.parametrize("change", ["reviewer", "date", "precision", "note"])
def test_policy_application_cannot_impersonate_another_policy_approval(
    adoption_case: AdoptionCase, change: str
) -> None:
    case = adoption_case
    original = case.replayed[0].decision
    fields = {
        "reviewer": {"reviewed_by": "Different reviewer"},
        "date": {"reviewed_at": "2026-10-01T00:00:00Z"},
        "precision": {"reviewed_precision": "instant"},
        "note": {"note": "Synthetic per-card answer"},
    }
    decision = original.model_copy(update=fields[change])
    snapshot = dataclasses.replace(case.snapshot, decisions={decision.id: decision})
    with pytest.raises(ValueError, match="impersonates"):
        replay_adoptions(
            snapshot, Reconstructor(case.root, {"wording-store": case.store})
        )


def test_existing_physical_observation_cannot_be_rebound_to_another_revision(
    adoption_case: AdoptionCase,
) -> None:
    case = adoption_case
    with create_database(compile_build(("t0", "semantics"))) as db:
        with db.transaction():
            case.base.stage(db)
        importer.import_adoptions(db, **case.inputs())
        item = case.replayed[0].selected
        changed = item.model_copy(
            update={
                "content": item.content.model_copy(
                    update={"effect": "Different synthetic content"}
                )
            }
        )
        with pytest.raises(ValueError, match="existing raw observation"):
            importer._physical(db, changed)


@pytest.mark.parametrize(
    "change", ["effect", "section", "stat", "type", "trait", "special"]
)
def test_an_existing_revision_id_does_not_hide_changed_content(
    adoption_case: AdoptionCase, change: str
) -> None:
    case = adoption_case
    schema = compile_build(("t0", "semantics"))
    with create_database(schema) as db:
        with db.transaction():
            case.base.stage(db)
        importer.import_adoptions(db, **case.inputs())
        item = case.replayed[0].selected
        identifier = candidate_revision_id(item)
        revision = next(
            r.values for r in db.rows("face_revision") if r.values["id"] == identifier
        )
        with db.transaction():
            if change == "effect":
                db.update(
                    "face_revision",
                    {"id": identifier},
                    {"effect_unit_id": revision["name_unit_id"]},
                )
            elif change == "stat":
                db.update("face_revision", {"id": identifier}, {"cost": 2})
            elif change == "type":
                db.update(
                    "face_revision", {"id": identifier}, {"class_code": "runecraft"}
                )
            elif change == "section":
                db.insert(
                    "face_text_section",
                    {
                        "revision_id": identifier,
                        "ordinal": 0,
                        "text_unit_id": revision["name_unit_id"],
                        "kind": "unknown",
                        "decision_id": None,
                    },
                )
            elif change == "trait":
                db.insert(
                    "face_trait", {"revision_id": identifier, "trait_code": "mage"}
                )
            else:
                db.insert(
                    "vocabulary",
                    {
                        "kind": "special_kind",
                        "code": "token",
                        "active": True,
                        "label_unit_id": revision["name_unit_id"],
                    },
                )
                db.insert(
                    "face_special_kind",
                    {"revision_id": identifier, "special_kind_code": "token"},
                )
        with pytest.raises(ValueError, match="content graph"):
            importer._verify_revision(
                db,
                item,
                next(
                    r.values
                    for r in db.rows("face_revision")
                    if r.values["id"] == identifier
                ),
                case.vocabulary,
            )


@pytest.mark.parametrize(
    "change", ["hash", "section-closure", "unknown-section", "reference", "unmapped"]
)
def test_semantics_preserve_unknown_sections_and_refuse_unproven_dsl_reuse(
    adoption_case: AdoptionCase, change: str
) -> None:
    case = adoption_case
    with create_database(compile_build(("t0", "semantics"))) as db:
        with db.transaction():
            case.base.stage(db)
        importer.import_adoptions(db, **case.inputs())
        semantic = db.rows("face_semantics")[0].values
        identifier = candidate_revision_id(case.replayed[0].selected)
        assert bundle_ready(db, identifier)
        if change == "unmapped":
            assert not bundle_ready(db, "missing")
            return
        if change == "section-closure":
            with (
                pytest.raises(sqlite3.IntegrityError, match="semantic_section_closure"),
                db.transaction(),
            ):
                db.update(
                    "face_semantics",
                    {"id": semantic["id"]},
                    {"rule_sections": Json(["missing"])},
                )
            return
        if change == "hash":
            with pytest.raises(ValueError, match="rule hash"), db.transaction():
                db.update(
                    "face_semantics",
                    {"id": semantic["id"]},
                    {"rule_hash": digest(b"wrong")},
                )
            return
        with db.transaction():
            if change == "unknown-section":
                db.insert(
                    "face_text_section",
                    {
                        "revision_id": identifier,
                        "ordinal": 0,
                        "text_unit_id": semantic["rule_text_unit_id"],
                        "kind": "unknown",
                        "decision_id": None,
                    },
                )
            else:
                db.insert(
                    "semantic_reference",
                    {
                        "semantic_id": semantic["id"],
                        "target_face_id": case.face_id,
                        "relation": "token_definition",
                    },
                )
        assert not bundle_ready(db, identifier)
