"""Four independent synthetic EN rule corrections exercise the deferred production scope."""

from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.manifest import Region
from sve_carddb.registry.records import CorrectionData
from sve_carddb.registry.review import Correction
from sve_carddb.snapshot.values import digest
from sve_carddb.source_archive import seal_batch
from sve_carddb.source_corrections import FrozenImages
from sve_carddb.source_corrections.images import evidence_url
from sve_carddb.source_corrections.projection import correction_references
from sve_carddb.text_observations import (
    Binding,
    Vocabulary,
    import_text_observations,
    plan_text_observations,
)

from .test_registry import card
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture
from .test_source_archive import _put, _resource, _store
from .text_observation_fixtures import make_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.review import Inputs


def test_four_synthetic_english_corrections_have_distinct_evidence_and_rule_results(
    tmp_path: Path, inputs: Inputs
) -> None:
    numbers = ("BP02-070EN", "BP02-072EN", "BP02-073EN", "BP02-074EN")
    raw_images = {}
    for index, number in enumerate(numbers):
        if number not in inputs.en:
            inputs.en[number] = card(
                number, "Synthetic name " + str(index), english=True
            )
            inputs.mapping.targets[number] = None
        face = inputs.en[number].faces[0]
        face.info["Card Type"] = "Spell"
        face.image = "/images/synthetic/" + number + ".png"
        raw_images[number] = b"\x89PNG\r\n\x1a\nSynthetic EN image " + number.encode()
    inputs.decisions.corrections = [
        Correction(
            region="en",
            card_no=number,
            field="card_type",
            expected_raw_value="Spell",
            corrected_value="Follower",
            image_sha256=digest(raw_images[number]),
            locator="Synthetic type box",
            state="active",
            reason="Synthetic EN type transcription",
        )
        for number in numbers
    ]
    case = make_case(tmp_path / "authored", inputs)
    case.vocabulary = Vocabulary(
        bindings=(
            *case.vocabulary.bindings,
            Binding(region="en", kind="type", raw="Spell", code="spell"),
        )
    )
    store = replace(_store(tmp_path / "images"), store_id="image-store")
    for record in case.identity.snapshot.records.values():
        if isinstance(record.data, CorrectionData):
            evidence = record.data.evidence[0]
            raw = next(
                raw for raw in raw_images.values() if digest(raw) == evidence.sha256
            )
            _put(
                store,
                replace(
                    _resource(
                        evidence_url(evidence),
                        "images/" + evidence.sha256.removeprefix("sha256:") + ".png",
                        raw,
                    ),
                    region=Region.EN,
                ),
                raw,
            )
    batch = seal_batch(store)
    images = FrozenImages(store.root, store.store_id, batch.batch_id)
    case.plan = plan_text_observations(case.identity, case.provider, images=images)
    assert case.plan.corrections is not None
    assert len(case.plan.corrections) == 4
    assert all(application.status == "applied" for application in case.plan.corrections)
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
        assert len(db.rows("source_correction")) == 4
        assert len(db.rows("correction_evidence")) == 4
        assert len(db.rows("correction_application")) == 4
        assert (
            len({row.values["source_id"] for row in db.rows("correction_evidence")})
            == 4
        )
        revisions = {row.values["id"]: row.values for row in db.rows("face_revision")}
        for row in db.rows("correction_application"):
            assert row.values["result_unit_id"] is None
            assert revisions[row.values["face_revision_id"]]["type_code"] == "follower"
            assert revisions[row.values["face_revision_id"]]["region"] == "en"
        assert len(correction_references(db, case.plan, case.vocabulary)) == 4
