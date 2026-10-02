"""Small defensive replay, freshness and semantic boundary counterexamples."""

import dataclasses
import re
from contextlib import nullcontext
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import Json, create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.snapshot.values import canonical
from sve_carddb.text_observations.intern import TextInterner
from sve_carddb.text_observations.models import candidate_revision_id
from sve_carddb.wording_adoptions import importer, replay, semantics
from sve_carddb.wording_adoptions.models import AdoptionRecord
from sve_carddb.wording_adoptions.reconstruction import Reconstructor

from .test_wording_adoption_integration import adoption_case as adoption_case  # ruff: ignore[useless-import-alias] -- shared immutable synthetic inputs
from .test_wording_historical_corrections import history as history  # ruff: ignore[useless-import-alias] -- shared immutable correction inputs
from .test_wording_loader_boundaries import human, successor
from .wording_adoption_fixtures import make_adoption_case

if TYPE_CHECKING:
    from sve_carddb.build_inputs import SourceUse
    from sve_carddb.registry.snapshot import RegistrySnapshot
    from sve_carddb.text_observations.models import FaceObservation
    from sve_carddb.wording_adoptions.models import Observation, ReviewContext
    from sve_carddb.wording_adoptions.reconstruction import ReconstructedScope

    from .text_observation_fixtures import Case
    from .wording_adoption_fixtures import AdoptionCase


@pytest.mark.parametrize("change", ["unknown", "level", "closure"])
def test_replayed_inventory_refuses_each_content_violation(
    adoption_case: AdoptionCase, change: str
) -> None:
    scope = adoption_case.scope
    record = adoption_case.replayed[0].record
    if change == "closure":
        record = record.model_copy(update={"evidence": ()})
    else:
        key = next(iter(scope.contents))
        content = scope.contents[key]
        changed = content.model_copy(
            update={
                "content": content.content.model_copy(
                    update={
                        "effect": None
                        if change == "unknown"
                        else "Synthetic changed rule"
                    }
                )
            }
        )
        scope = dataclasses.replace(
            scope, contents=dict(scope.contents) | {key: changed}
        )
    message = {
        "unknown": "Unknown effect cannot confirm wording equivalence",
        "level": "One wording level contains different reconstructed contents",
        "closure": "Adoption evidence omits raw, presence or correction closure",
    }[change]
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        replay._membership(record, scope)


@pytest.mark.parametrize("change", ["unreplayed", "null", "policy", "identity"])
def test_replay_refuses_missing_predecessor_and_invalid_review(
    adoption_case: AdoptionCase, change: str
) -> None:
    case = adoption_case
    record = case.replayed[0].record
    prior = case.replayed[0].previous
    reconstruction = Reconstructor(case.root, {"wording-store": case.store})
    if change == "unreplayed":
        record = successor(record, case.replayed[0].decision.id)
    elif change == "null":
        wire = record.model_dump(mode="json")
        wire["data"]["previous"] = None
        wire["data"]["previous_order"] = None
        record = AdoptionRecord.model_validate_json(canonical(wire))
    elif change == "policy":
        record = record.model_copy(
            update={
                "data": record.data.model_copy(
                    update={
                        "review": record.data.review.model_copy(
                            update={"rule_set": None}
                        )
                    }
                )
            }
        )
    else:
        record = human(record)
        prior = case.replayed[0].previous
        assert prior is not None
        prior = prior.model_copy(
            update={
                "content": prior.content.model_copy(
                    update={"name": "Synthetic different identity"}
                )
            }
        )
    message = {
        "unreplayed": "Adoption predecessor has not been replayed",
        "null": "Null predecessor cannot discard an available mechanical current",
        "policy": "Approved-rule adoption has no policy pin",
        "identity": "Wording equivalence cannot override rule dependencies or identity",
    }[change]

    def operation() -> None:
        if change in {"policy", "identity"}:
            replay._review(
                record,
                case.scope,
                case.replayed[0].decision,
                prior if change == "identity" else case.replayed[0].previous,
                reconstruction,
            )
        else:
            snapshot = dataclasses.replace(
                case.snapshot, records={record.record_key: record}
            )
            replay.replay_adoptions(snapshot, reconstruction)

    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        operation()


@pytest.mark.parametrize("change", ["text", "correction-key"])
def test_known_current_correction_change_invalidates_adopted_wording(
    adoption_case: AdoptionCase, monkeypatch: pytest.MonkeyPatch, change: str
) -> None:
    case = adoption_case
    prepared = importer.prepare_adoptions(case.inputs(newer=True))
    original = prepared.reconstruction.rebuild_observation

    def corrected(
        registry: RegistrySnapshot, review: ReviewContext, raw: FaceObservation
    ) -> tuple[Observation, FaceObservation, tuple[SourceUse, ...]]:
        observation, item, uses = original(registry, review, raw)
        if change == "text":
            item = item.model_copy(
                update={
                    "content": item.content.model_copy(
                        update={"effect": "Synthetic changed correction"}
                    )
                }
            )
        else:
            item = item.model_copy(update={"correction_keys": ("sha256:" + "0" * 64,)})
        return observation, item, uses

    monkeypatch.setattr(prepared.reconstruction, "rebuild_observation", corrected)
    with create_database(compile_build(("t0", "semantics"))) as db:
        with db.transaction():
            case.base.stage(db)
        importer.import_adoptions(db, **case.inputs())
        before = db.rows("face_current")
        with pytest.raises(
            ValueError,
            match=r"\AKnown current correction change invalidates adopted wording\Z",
        ):
            importer._freshness(db, prepared, case.newer_review)
        assert db.rows("face_current") == before


def test_missing_rule_text_is_not_an_empty_semantic_rule(
    adoption_case: AdoptionCase,
) -> None:
    content = adoption_case.replayed[0].selected.content.model_copy(
        update={"effect": None}
    )
    with pytest.raises(
        ValueError, match=r"\AMissing rule text cannot create semantics\Z"
    ):
        semantics.rule_hash(content)


def test_semantic_population_requires_all_three_tables(
    adoption_case: AdoptionCase,
) -> None:
    with create_database(compile_build(("t0",))) as db:
        with pytest.raises(
            ValueError,
            match=r"\AConfirmed wording adoption requires the semantics capability\Z",
        ):
            semantics.populate_semantics(
                db, adoption_case.replayed[0], TextInterner(db, published=())
            )


@pytest.mark.parametrize("change", ["immutable", "incompatible", "dependency"])
def test_semantic_graph_refusals_are_independent(
    adoption_case: AdoptionCase, human_case: AdoptionCase, change: str
) -> None:
    case = human_case if change == "incompatible" else adoption_case
    adoption = case.replayed[0]
    with create_database(compile_build(("t0", "semantics"))) as db:
        with db.transaction():
            case.base.stage(db)
        importer.import_adoptions(db, **case.inputs())
        semantic = db.rows("face_semantics")[0].values

        def operation() -> None:
            if change == "immutable":
                for row in db.rows("revision_semantics"):
                    db.delete(
                        "revision_semantics", {"revision_id": row.values["revision_id"]}
                    )
                db.update(
                    "face_semantics",
                    {"id": semantic["id"]},
                    {"normalizer_version": "Synthetic conflicting representation"},
                )
            elif change == "incompatible":
                # Distinct semantic IDs can pass FKs without proving equivalence.
                db.insert(
                    "face_semantics", dict(semantic) | {"id": "sem:v1:" + "0" * 64}
                )
                prior = adoption.previous
                assert prior is not None
                assert candidate_revision_id(
                    adoption.selected
                ) != candidate_revision_id(prior)
                db.update(
                    "revision_semantics",
                    {"revision_id": candidate_revision_id(adoption.selected)},
                    {"semantic_id": "sem:v1:" + "0" * 64},
                )
            else:
                db.update(
                    "face_semantics",
                    {"id": semantic["id"]},
                    {"rule_sections": Json(["synthetic-missing-unit"])},
                )
                semantics.verify_semantics(db)
                return
            semantics.populate_semantics(db, adoption, TextInterner(db, published=()))

        with (
            pytest.raises(
                ValueError if change != "dependency" else TypeError,
                match=rf"\A{re.escape({'immutable': 'Immutable semantic representation conflicts', 'incompatible': 'Checked revision already has incompatible semantics', 'dependency': 'Semantic section text dependency is missing'}[change])}\Z",
            ),
            db.transaction(),
        ):
            operation()


@pytest.fixture(scope="module")
def human_case(tmp_path_factory: pytest.TempPathFactory) -> AdoptionCase:
    return make_adoption_case(
        tmp_path_factory.mktemp("human-runtime-boundary"), human=True
    )


@pytest.mark.parametrize("change", ["envelope", "registry", "capability", "ordinal"])
def test_importer_refusals_remain_distinct(
    adoption_case: AdoptionCase, change: str
) -> None:
    case = adoption_case
    inputs = case.inputs()
    if change == "registry":
        inputs["registry"] = dataclasses.replace(
            inputs["registry"],
            files=dataclasses.replace(
                inputs["registry"].files,
                index_content=b"Synthetic different registry index",
            ),
        )
    message = {
        "envelope": "Adoption decision is already imported or has conflicting identity",
        "registry": "Current wording review and build registry disagree",
        "capability": "Wording adoption requires enabled semantics capability",
        "ordinal": "Section ordinal must be an integer",
    }[change]
    if change == "ordinal":
        with pytest.raises(TypeError, match=rf"\A{re.escape(message)}\Z"):
            importer._ordinal("0")
        return
    if change == "registry":
        with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
            importer.prepare_adoptions(inputs)
        return
    capabilities = ("t0",) if change == "capability" else ("t0", "semantics")
    with create_database(compile_build(capabilities)) as db:
        with db.transaction():
            case.base.stage(db)
        if change == "envelope":
            importer.import_adoptions(db, **inputs)

        def operation() -> None:
            if change == "envelope":
                # Another shard can reuse a decision while having a distinct source ID.
                shard = dataclasses.replace(
                    case.snapshot.shards[0], path="wording-adoptions/jp/002.yaml"
                )
                importer._envelopes(
                    db, dataclasses.replace(case.snapshot, shards=(shard,))
                )
            else:
                importer.import_adoptions(db, **inputs)

        with (
            pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"),
            db.transaction() if change == "envelope" else nullcontext(),
        ):
            operation()


def test_evidence_cannot_borrow_an_unreviewed_but_sealed_batch(
    adoption_case: AdoptionCase,
) -> None:
    case = adoption_case
    record = case.replayed[0].record
    extra = record.evidence[0].model_copy(
        update={
            "batch_id": case.newer_review.source_batches[0].batch_id,
            "role": "synthetic_extra",
        }
    )
    wire = record.model_dump(mode="json")
    wire["evidence"] = [
        e.model_dump(mode="json")
        for e in sorted(
            (*record.evidence, extra),
            key=lambda e: canonical(e.model_dump(mode="json")),
        )
    ]
    record = AdoptionRecord.model_validate_json(canonical(wire))
    snapshot = dataclasses.replace(case.snapshot, records={record.record_key: record})
    with pytest.raises(
        ValueError,
        match=r"\AEvidence batch is outside its corresponding reviewed contexts\Z",
    ):
        replay.replay_adoptions(
            snapshot, Reconstructor(case.root, {"wording-store": case.store})
        )


def test_independent_predecessor_answer_must_bind_both_selected_keys(
    human_case: AdoptionCase,
) -> None:
    case = human_case
    wire = case.replayed[0].record.model_dump(mode="json")
    wire["data"]["previous_order"]["review_receipt"]["before_observation_keys"] = [
        "synthetic-wrong-key"
    ]
    record = AdoptionRecord.model_validate_json(canonical(wire))
    snapshot = dataclasses.replace(case.snapshot, records={record.record_key: record})
    with pytest.raises(
        ValueError,
        match=r"\APredecessor order answer does not bind both selected keys\Z",
    ):
        replay.replay_adoptions(
            snapshot, Reconstructor(case.root, {"wording-store": case.store})
        )


def test_historical_correction_cannot_materialize_unknown_rule_text(
    history: tuple[Case, ReconstructedScope],
) -> None:
    case, scope = history
    original = scope.contents["new"]
    item = original.model_copy(
        update={"content": original.content.model_copy(update={"effect": None})}
    )
    scope = dataclasses.replace(scope, contents={"new": item})
    with create_database(
        compile_build(("t0", "semantics", "correction", "en", "related"))
    ) as db:
        with db.transaction():
            case.stage(db)
        with (
            pytest.raises(
                ValueError,
                match=r"\AHistorical correction has no representable result revision\Z",
            ),
            db.transaction(),
        ):
            importer._correction_history(db, scope, TextInterner(db, published=()))
