"""Historical correction hashes bind the absence projection before correction."""

import dataclasses
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import BuildContext, insert_raw_sources
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.source_corrections.plan import (
    corrected_observations,
    historical_application,
)
from sve_carddb.text_observations.importer import (
    populate_revision,
    populate_text_observations,
)
from sve_carddb.text_observations.intern import TextInterner
from sve_carddb.wording_adoptions.importer import _correction_history
from sve_carddb.wording_adoptions.models import ReviewContext
from sve_carddb.wording_adoptions.reconstruction import (
    ReconstructedScope,
    Reconstructor,
)

from .registry_snapshot_fixtures import edit_record
from .source_correction_fixtures import make_correction_case
from .test_effect_presence import card_from_raw, page
from .test_registry import make_inputs

if TYPE_CHECKING:
    from sve_carddb.registry.snapshot import RegistrySnapshot
    from sve_carddb.registry.storage import Entry
    from sve_carddb.text_observations.models import FaceObservation

    from .text_observation_fixtures import Case


@pytest.fixture(scope="module")
def corrected(
    tmp_path_factory: pytest.TempPathFactory,
) -> tuple[
    Reconstructor, RegistrySnapshot, ReviewContext, FaceObservation, FaceObservation
]:
    root = tmp_path_factory.mktemp("wording-correction")
    fixture = make_correction_case(
        root / "authored", make_inputs(), region="en", field="card_type"
    )
    card = card_from_raw(
        root / "raw",
        page("en")
        .replace(b"Synthetic type", b"Spell")
        .replace(b"SYN-01", b"BP02-070EN"),
        "en",
        number="BP02-070EN",
    )

    def edit(entry: Entry) -> None:
        entry.data["expected_source_hash"] = card.observation.observation_hash

    edit_record(fixture.texts.root, "source_correction", edit)
    registry = load_registry(fixture.texts.root)
    assert fixture.texts.plan.corrections is not None
    application = fixture.texts.plan.corrections[0]
    item = application.observation.model_copy(
        update={"card": card, "content": card.projected(0)}
    )
    fixed = card_from_raw(
        root / "fixed",
        page("en")
        .replace(b"Synthetic type", b"Follower")
        .replace(b"SYN-01", b"BP02-070EN"),
        "en",
        number="BP02-070EN",
    )
    already = item.model_copy(update={"card": fixed, "content": fixed.projected(0)})
    batch = fixture.images.sources.batch_id
    review = ReviewContext.model_validate_json(
        canonical(
            {
                "context": BuildContext.from_inputs(
                    "1" * 40, {"synthetic.lock": b"synthetic"}, {}
                ).model_dump(mode="json"),
                "source_batches": [{"store_id": "image-store", "batch_id": batch}],
            }
        )
    )
    reconstruction = Reconstructor(root, {"image-store": fixture.image_store})
    return reconstruction, registry, review, item, already


@pytest.mark.parametrize("status", ["applied", "already_fixed"])
def test_absent_then_correction_reproduces_every_receipt_field(
    corrected: tuple[
        Reconstructor, RegistrySnapshot, ReviewContext, FaceObservation, FaceObservation
    ],
    status: str,
) -> None:
    reconstruction, registry, review, item, already = corrected
    raw = item if status == "applied" else already
    observation, result, uses = reconstruction.rebuild_observation(
        registry, review, raw
    )
    assert observation.effect_presence.result.state == "absent"
    assert raw.card.faces[0].effect is None
    assert result.content.effect is not None
    assert not result.content.effect
    assert result.content.type_raw == "Follower"
    assert observation.raw_face_hash == raw.card.faces[0].fingerprint()
    assert (
        observation.content_hash
        == raw.content.model_copy(update={"type_raw": "Follower"}).fingerprint()
    )
    assert observation.corrections[0].status == status
    assert result.correction_keys == (
        (observation.corrections[0].record_hash,) if status == "applied" else ()
    )
    assert {u.usage for u in uses} == {
        "source_correction_comparison",
        "source_correction_evidence",
    }


@pytest.mark.parametrize("change", ["conflict", "hash", "image", "decision", "errata"])
def test_historical_correction_cannot_borrow_or_ignore_its_confirmed_evidence(
    corrected: tuple[
        Reconstructor, RegistrySnapshot, ReviewContext, FaceObservation, FaceObservation
    ],
    change: str,
) -> None:
    reconstruction, registry, review, item, _ = corrected
    if change == "conflict":
        item = item.model_copy(
            update={
                "content": item.content.model_copy(update={"type_raw": "Different"})
            }
        )
    elif change == "hash":
        item = item.model_copy(
            update={
                "card": item.card.model_copy(
                    update={
                        "observation": item.card.observation.model_copy(
                            update={"observation_hash": "sha256:" + "0" * 64}
                        )
                    }
                )
            }
        )
    elif change == "image":
        review = review.model_copy(update={"source_batches": ()})
    elif change == "decision":
        decisions = {
            k: d.model_copy(update={"state": "proposed"})
            for k, d in registry.decisions.items()
        }
        registry = dataclasses.replace(registry, decisions=decisions)
    else:
        item = item.model_copy(
            update={"card": item.card.model_copy(update={"has_errata_link": True})}
        )
    with pytest.raises(ValueError, match=r"conflicting|confirmed|closure|errata"):
        reconstruction.rebuild_observation(registry, review, item)


@pytest.fixture(scope="module")
def history(
    tmp_path_factory: pytest.TempPathFactory,
) -> tuple[Case, ReconstructedScope]:
    root = tmp_path_factory.mktemp("correction-history-db")
    fixture = make_correction_case(root, make_inputs(), region="en", field="card_type")
    card = card_from_raw(
        root / "next",
        page("en")
        .replace(b"Synthetic type", b"Follower")
        .replace(b"SYN-01", b"BP02-070EN"),
        "en",
        number="BP02-070EN",
    )
    assert fixture.texts.plan.corrections is not None
    previous = fixture.texts.plan.corrections[0]
    raw = previous.observation.model_copy(
        update={"card": card, "content": card.projected(0)}
    )
    application = historical_application(
        fixture.texts.identity.snapshot, previous.record, raw, fixture.images
    )
    item = corrected_observations((raw,), (application,))[0]
    scope = ReconstructedScope(
        fixture.texts.identity.snapshot,
        None,
        (),
        {"new": item},
        application.uses(),
        {},
        (application,),
    )
    return fixture.texts, scope


@pytest.mark.parametrize("change", ["none", "capability", "provenance"])
def test_historical_applications_keep_their_own_raw_and_corrected_revision(
    history: tuple[Case, ReconstructedScope], change: str
) -> None:
    case, scope = history
    schema = compile_build(
        ("t0", "semantics")
        if change == "capability"
        else ("t0", "semantics", "correction", "en", "related")
    )
    with create_database(schema) as db:
        if change == "capability":
            with (
                pytest.raises(ValueError, match="correction capability"),
                db.transaction(),
            ):
                _correction_history(db, scope, TextInterner(db, published=()))
            return
        with db.transaction():
            case.stage(db)
            populate_text_observations(
                db,
                case.plan,
                build=case.context(),
                vocabulary=case.vocabulary,
                published=(),
            )
        item = scope.contents["new"]
        with db.transaction():
            insert_raw_sources(db, (item.card.source,))
            texts = TextInterner(db, published=())
            populate_revision(db, item, 99, texts, case.vocabulary)
        if change == "provenance":
            with db.transaction():
                db.update(
                    "source_correction",
                    {"id": scope.applications[0].data.id},
                    {"expected_source_hash": digest(b"wrong")},
                )
            with pytest.raises(ValueError, match="provenance"), db.transaction():
                _correction_history(db, scope, TextInterner(db, published=()))
        else:
            with db.transaction():
                _correction_history(db, scope, TextInterner(db, published=()))
            rows = db.rows("correction_application")
            assert len(rows) == 2
            assert {r.values["status"] for r in rows} == {"applied", "already_fixed"}
            assert len({r.values["source_id"] for r in rows}) == 2
