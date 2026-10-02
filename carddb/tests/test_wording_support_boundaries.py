"""Non-vacuous support and newly changed candidate assertions after adoption."""

from typing import TYPE_CHECKING

from sve_carddb.build_db import Json, create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import BuildContext
from sve_carddb.manifest import Kind
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.text_observations.importer import populate_text_observations
from sve_carddb.text_observations.models import candidate_revision_id
from sve_carddb.text_observations.wording import wording_views
from sve_carddb.wording_adoptions import importer
from sve_carddb.wording_adoptions.models import ReviewContext
from sve_carddb.wording_adoptions.reconstruction import Reconstructor

from .test_effect_presence import page
from .test_source_archive import _put, _resource, _store
from .test_wording_adoption_integration import adoption_case as adoption_case  # ruff: ignore[useless-import-alias] -- shared immutable synthetic inputs

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database

    from .wording_adoption_fixtures import AdoptionCase


def stage(db: Database, case: AdoptionCase) -> None:
    with db.transaction():
        case.base.stage(db)
        populate_text_observations(
            db,
            case.base.plan,
            build=case.base.context(),
            vocabulary=case.base.vocabulary,
            published=(),
        )


def test_adoption_removes_only_pending_and_keeps_automatic_support_disabled(
    adoption_case: AdoptionCase,
) -> None:
    case = adoption_case
    with create_database(compile_build(("t0", "semantics"))) as db:
        stage(db, case)
        card = next(
            r.values["card_id"]
            for r in db.rows("face")
            if r.values["id"] == case.face_id
        )
        with db.transaction():
            db.insert(
                "card_engine_support",
                {
                    "card_id": card,
                    "region": "jp",
                    "status": "missing_dsl",
                    "dsl_id": None,
                    "candidate_hash": None,
                    "dsl_version": None,
                    "engine_version": None,
                    "engine_build_hash": None,
                    "validation_policy_id": None,
                    "load_id": None,
                    "validation_state": "not_applicable",
                    "reason_codes": Json(["missing_dsl", "wording_pending"]),
                    "automatic": False,
                },
            )
        importer.import_adoptions(db, **case.inputs())
        required = {
            r.values["id"] for r in db.rows("face") if r.values["card_id"] == card
        }
        currents = {
            r.values["face_id"]
            for r in db.rows("face_current")
            if r.values["region"] == "jp"
        }
        assert required
        assert required <= currents
        rows = db.rows("card_engine_support")
        assert len(rows) == 1
        assert rows[0].values["reason_codes"] == Json(["missing_dsl"])
        assert rows[0].values["automatic"] is False


def test_new_changed_source_candidate_comes_from_current_adoption_inventory(  # ruff: ignore[too-many-locals] -- one isolated frozen version pins every current-review dependency
    adoption_case: AdoptionCase, tmp_path: Path
) -> None:
    case = adoption_case
    chosen = next(
        i
        for i in case.scope.contents.values()
        if i.printing_id != case.replayed[0].selected.printing_id
    )
    store = _store(tmp_path / "new-source")
    raw = (
        page("jp", '<div class="detail">Synthetic unchecked changed text</div>')
        .replace(b"SYN-01", chosen.card_no.encode())
        .replace(b"Synthetic type", "フォロワー".encode())
    )
    _put(
        store,
        _resource(card_url(chosen.card_no), "raw/changed.html", raw, Kind.CARD),
        raw,
    )
    batch = seal_batch(store).batch_id
    wire = case.review.model_dump(mode="json")
    wire["source_batches"].append({"store_id": store.store_id, "batch_id": batch})
    wire["source_batches"].sort(key=canonical)
    review = ReviewContext.model_validate_json(canonical(wire))
    stores = {"wording-store": case.store, store.store_id: store.root}
    scope = Reconstructor(case.root, stores).scope(review, case.face_id, "jp")
    changed = next(
        i
        for i in scope.contents.values()
        if i.card.source.archive.store_id == store.store_id
    )
    expected = candidate_revision_id(changed)
    assert expected != candidate_revision_id(case.replayed[0].selected)
    inputs = case.inputs()
    dependencies = importer.adoption_dependencies(case.snapshot, case.replayed)
    dependencies.update(
        {k: v for k, v in case.scope.dependencies.items() if k.startswith("carddb/")}
    )
    key = digest(canonical(review.model_dump(mode="json"))).removeprefix("sha256:")
    dependencies.update(
        {f"wording-reviews/{key}/{k}": v for k, v in scope.dependencies.items()}
    )
    configuration = object_value(parse(inputs["build"].configuration.encode()))
    configuration["wording_current_review"] = review.model_dump(mode="json")
    inputs["stores"] = stores
    inputs["current_review"] = review
    inputs["build"] = BuildContext.from_inputs(
        review.context.program_revision, dependencies, configuration
    )
    with create_database(compile_build(("t0", "semantics"))) as db:
        stage(db, case)
        importer.import_adoptions(db, **inputs)
        view = wording_views(db, case.base.plan)[case.face_id][0]
        assert (chosen.printing_id, expected) in {
            (c.printing_id, c.revision_id) for c in view.candidates
        }
        assert view.display.revision_id == candidate_revision_id(
            case.replayed[0].selected
        )
        assert expected not in {
            r.values["revision_id"] for r in db.rows("revision_semantics")
        }
