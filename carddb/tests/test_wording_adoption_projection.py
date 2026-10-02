"""Validate both regional histories before projecting a JP-only build."""

import dataclasses
import shutil
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import BuildContext
from sve_carddb.manifest import Kind, Region
from sve_carddb.registry.records import PrintingData
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources.official_en import card_url
from sve_carddb.wording_adoptions import importer
from sve_carddb.wording_adoptions.loader import load_adoptions
from sve_carddb.wording_adoptions.models import AdoptionRecord, ReviewContext
from sve_carddb.wording_adoptions.reconstruction import Reconstructor, scope_evidence
from sve_carddb.wording_adoptions.replay import replay_adoptions

from .test_effect_presence import page
from .test_source_archive import _put, _resource, _store
from .wording_adoption_fixtures import commit, install_adoptions, make_adoption_case

if TYPE_CHECKING:
    from pathlib import Path

    from .wording_adoption_fixtures import AdoptionCase


@pytest.fixture(scope="module")
def regional_case(  # ruff: ignore[too-many-locals] -- shared fixture pins two independent regional histories once
    tmp_path_factory: pytest.TempPathFactory,
) -> tuple[AdoptionCase, Path]:
    root = tmp_path_factory.mktemp("wording-regions")
    case = make_adoption_case(root)
    store = dataclasses.replace(_store(root / "english"), store_id="en-wording-store")
    printings = tuple(
        r.data
        for r in case.scope.registry.records.values()
        if isinstance(r.data, PrintingData)
        and r.data.region == "en"
        and any(m.face_id == case.face_id for m in r.data.source_face_map)
    )
    assert printings
    for i, printing in enumerate(printings):
        raw = (
            page(
                "en",
                '<div class="detail">Synthetic paragraph</div>',
                double=len(printing.source_face_map) == 2,
            )
            .replace(b"SYN-01", printing.card_no.encode())
            .replace(b"Synthetic type", b"Follower")
        )
        resource = dataclasses.replace(
            _resource(card_url(printing.card_no), f"raw/{i}.html", raw, Kind.CARD),
            region=Region.EN,
        )
        _put(store, resource, raw)
    batch = seal_batch(store).batch_id
    wire = case.review.model_dump(mode="json")
    configuration = object_value(parse(case.review.context.configuration.encode()))
    configuration["regions"] = ["en"]
    wire["source_batches"] = [{"store_id": store.store_id, "batch_id": batch}]
    # Configuration is part of the context hash, including its own identity.
    context = case.review.context.from_inputs(
        case.review.context.program_revision,
        case.scope.dependencies,
        configuration,
    )
    wire["context"] = context.model_dump(mode="json")
    review = ReviewContext.model_validate_json(canonical(wire))
    stores = {"wording-store": case.store, store.store_id: store.root}
    scope = Reconstructor(case.root, stores).scope(review, case.face_id, "en")
    selected = min(
        scope.contents,
        key=lambda k: (
            scope.contents[k].card.source.id,
            scope.contents[k].printing_id,
        ),
    )
    observations: list[JsonValue] = [
        o.model_dump(mode="json") for o in scope.observations
    ]
    keys = [o.observation_key for o in scope.observations]
    record = case.replayed[0].record.model_dump(mode="json")
    record["record_key"] = canonical(
        ["wording_adoption", case.face_id, "en", 1]
    ).decode()
    record["filing_key"] = "en"
    record["data"].update(
        region="en",
        review_context=review.model_dump(mode="json"),
        observations=observations,
        observations_hash=digest(canonical(observations)),
        checked_observation_keys=keys,
        wording_order=[keys],
        selected_observation_key=selected,
        previous={
            "kind": "mechanical",
            "review_context": review.model_dump(mode="json"),
            "observations": observations,
            "observations_hash": digest(canonical(observations)),
            "selected_observation_key": selected,
        },
    )
    record["data"]["review"]["rule_matches"] = []
    record["evidence"] = [e.model_dump(mode="json") for e in scope_evidence(scope)]
    install_adoptions(
        case.root / "authored",
        [AdoptionRecord.model_validate_json(canonical(record))],
        region="en",
    )
    revision = commit(case.root)
    snapshot = load_adoptions(
        case.root / "authored",
        authored_revision=revision,
        registry=case.scope.registry,
        stores=stores,
    )
    replayed = replay_adoptions(snapshot, Reconstructor(case.root, stores))
    assert len(replayed) == 2
    return dataclasses.replace(case, snapshot=snapshot, replayed=replayed), store.root


def test_jp_build_validates_en_history_and_projects_only_jp(
    regional_case: tuple[AdoptionCase, Path],
) -> None:
    case, english = regional_case
    inputs = case.inputs()
    inputs["stores"] = {"wording-store": case.store, "en-wording-store": english}
    prepared = importer.prepare_adoptions(inputs)
    report = importer.adoption_report(prepared)
    assert len(prepared.replayed) == 2
    assert len(prepared.projected()) == 1
    assert len(array(report["validated_wording_adoption_keys"])) == 2
    with create_database(compile_build(("t0", "semantics"))) as db:
        with db.transaction():
            case.base.stage(db)
        importer.import_adoptions(db, **inputs)
        assert {r.values["region"] for r in db.rows("face_current")} == {"jp"}
        assert {r.values["region"] for r in db.rows("face_semantics")} == {"jp"}
        assert len(db.rows("revision_semantics")) > 0
        en_sources = {
            i.source.id
            for i in prepared.uses
            if i.source.archive.store_id == "en-wording-store"
        }
        assert en_sources
        assert en_sources <= {r.values["id"] for r in db.rows("source_record")}


def test_bad_en_policy_application_blocks_jp_build_before_any_write(
    regional_case: tuple[AdoptionCase, Path], tmp_path: Path
) -> None:
    case, english = regional_case
    root = tmp_path / "repository"
    shutil.copytree(case.root, root)
    shard_path = root / "authored/wording-adoptions/en/001.yaml"
    shard = object_value(read_yaml(shard_path))
    object_value(array(shard["decisions"])[0])["reviewed_by"] = "Different reviewer"
    shard_path.write_bytes(canonical(shard))
    index_path = root / "authored/wording-adoptions/index.yaml"
    index = object_value(read_yaml(index_path))
    object_value(index["includes"])["wording-adoptions/en/001.yaml"] = digest(
        canonical(shard)
    )
    index_path.write_bytes(canonical(index))
    revision = commit(root)
    stores = {"wording-store": case.store, "en-wording-store": english}
    snapshot = load_adoptions(
        root / "authored",
        authored_revision=revision,
        registry=case.scope.registry,
        stores=stores,
    )
    invalid = dataclasses.replace(case, root=root, snapshot=snapshot, inputs_cache={})
    inputs = invalid.inputs()
    inputs["stores"] = stores
    schema = compile_build(("t0", "semantics"))
    with create_database(schema) as db:
        with db.transaction():
            case.base.stage(db)
        before = {t.name: db.rows(t.name) for t in schema.tables}
        with pytest.raises(ValueError, match="policy approver"):
            importer.import_adoptions(db, **inputs)
        assert before == {t.name: db.rows(t.name) for t in schema.tables}


@pytest.mark.parametrize("change", ["order", "pin"])
def test_regional_projection_is_explicit_and_pinned(
    regional_case: tuple[AdoptionCase, Path], change: str
) -> None:
    case, english = regional_case
    inputs = case.inputs()
    inputs["stores"] = {"wording-store": case.store, "en-wording-store": english}
    if change == "order":
        inputs["regions"] = ("jp", "en")
    else:
        build = inputs["build"]
        configuration = object_value(parse(build.configuration.encode()))
        configuration["wording_adoption_regions"] = ["en"]
        inputs["build"] = BuildContext(
            program_revision=build.program_revision,
            dependencies=build.dependencies,
            configuration=canonical(configuration).decode(),
        )
    with pytest.raises(ValueError, match=r"projection regions|configuration"):
        importer.prepare_adoptions(inputs)
