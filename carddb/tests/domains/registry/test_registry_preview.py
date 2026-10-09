"""Regional projection preserves every record and verifies independent evidence."""

import json
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.domains.registry.preview import import_preview, plan_preview
from sve_carddb.domains.registry.preview.evidence import FaceEvidence
from sve_carddb.domains.registry.records import CorrectionData, PrintingData
from sve_carddb.domains.registry.storage import Shard, read_yaml

from ...support.registry_preview_fixtures import BUILD, REVISION, evidence, parents
from ...support.registry_snapshot_fixtures import edit_record, rewrite
from ...support.registry_snapshot_fixtures import registry_root as registry_root  # ruff: ignore[useless-import-alias] -- expose shared synthetic fixture
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- expose shared fixture dependency

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.registry.review import Inputs
    from sve_carddb.domains.registry.storage import Entry


def test_jp_projection_preserves_all_history_and_explains_en(
    registry_root: Path,
    inputs: Inputs,
) -> None:
    before = {p: p.read_bytes() for p in registry_root.rglob("*.yaml")}
    plan = plan_preview(registry_root, evidence(inputs, en=False), regions=("jp",))
    assert plan.regions == ("jp",)
    assert len(plan.included("printing")) == 2
    assert len(plan.included("card")) == 1
    assert not plan.included("card_related")
    assert not plan.included("art")
    assert not plan.included("region_mapping_review")
    assert len(plan.snapshot.records) == len(plan.projections)
    en = [p for p in plan.projections if "outside_output_regions" in p.reasons]
    assert len(en) == 2
    assert all("missing_source" in p.reasons for p in en)
    shared = next(
        p
        for p in plan.projections
        if p.record_key == plan.included("card")[0].record_key
    )
    assert shared.disposition == "included"
    assert any(check.status == "missing_source" for check in shared.evidence)
    assert plan.snapshot.files.index().next_int_id == {"jp": 20003, "en": 60003}
    corrections = [
        p
        for p in plan.projections
        if isinstance(plan.snapshot.records[p.record_key].data, CorrectionData)
    ]
    assert len(corrections) == 2
    assert all(p.disposition == "deferred" for p in corrections)
    assert all(p.reasons == ("source_correction_deferred",) for p in corrections)
    report = json.dumps(plan.report())
    assert "Corrected synthetic rule." not in report
    assert "Rule." not in report
    assert before == {p: p.read_bytes() for p in before}


def test_full_synthetic_import_links_en_art_and_reskin(
    registry_root: Path,
    inputs: Inputs,
) -> None:
    plan = plan_preview(registry_root, evidence(inputs), regions=("jp", "en"))
    assert len(plan.included("printing")) == 4
    assert len(plan.included("art")) == 1
    assert len(plan.included("card_related")) == 1
    assert len(plan.included("region_mapping_review")) == 1
    related = next(
        p for p in plan.projections if p.record_key.startswith("card_related:")
    )
    assert related.regions == ("en",)
    with create_database(compile_build(("en", "related"))) as db:
        parents(db, plan)
        import_preview(db, plan, build=BUILD, authored_revision=REVISION)
        assert len(db.rows("card")) == 2
        assert len(db.rows("printing")) == 4
        assert len(db.rows("printing_face")) == 4
        assert {r.values["int_id"] for r in db.rows("card_int_id")} == {
            20001,
            20002,
            60001,
            60002,
        }
        by_number = {r.values["card_no"]: r.values for r in db.rows("printing")}
        assert by_number["BP02-070EN"]["card_id"] == by_number["BP02-071"]["card_id"]
        assert by_number["GF01-001EN"]["card_id"] != by_number["BP02-071"]["card_id"]
        assert all(
            r.values["printed_text_state"] == "unknown"
            for r in db.rows("printing_face")
        )
        assert (
            sum(r.values["art_id"] is not None for r in db.rows("printing_face")) == 1
        )
        assert not db.rows("decision")
        assert not db.rows("decision_source")
        authored_sources = {
            r.values["id"]: r.values
            for r in db.rows("source_record")
            if r.values["kind"] == "authored"
        }
        assert len(authored_sources) == len(plan.snapshot.files.shards)
        assert all(
            r["authored_revision"] == REVISION for r in authored_sources.values()
        )
        mapping = db.rows("region_mapping_review")[0].values
        assert mapping["source_id"] in authored_sources
        assert sum(check.status == "matched" for check in related.evidence) == 4


@pytest.mark.parametrize(
    ("region", "number"), [("jp", "BP02-071"), ("en", "BP02-070EN")]
)
@pytest.mark.parametrize("field", ["text", "name", "image", "sections"])
def test_changed_source_excludes_reviewed_mapping_and_reskin(
    registry_root: Path,
    inputs: Inputs,
    region: str,
    number: str,
    field: str,
) -> None:
    card = (inputs.jp if region == "jp" else inputs.en)[number]
    setattr(card.faces[0], field, ["changed"] if field == "sections" else "changed")
    plan = plan_preview(registry_root, evidence(inputs), regions=("jp", "en"))
    assert "BP02-070EN" not in {
        r.data.card_no
        for r in plan.included("printing")
        if isinstance(r.data, PrintingData)
    }
    assert not plan.included("card_related")
    assert any("observation_mismatch" in p.reasons for p in plan.projections)
    if region == "en":
        assert not plan.included("art")


def test_unavailable_review_coverage_does_not_reconfirm_none(
    registry_root: Path, inputs: Inputs
) -> None:
    provider = replace(evidence(inputs), coverage_hashes=frozenset())
    plan = plan_preview(registry_root, provider, regions=("en",))
    item = next(
        p for p in plan.projections if p.record_key.startswith("region_mapping_review:")
    )
    assert item.disposition == "excluded"
    assert item.reasons == ("missing_review_coverage",)


def test_en_damage_fails_before_jp_projection(
    registry_root: Path, inputs: Inputs
) -> None:
    def damage(entry: Entry) -> None:
        entry.data["source_face_map"] = []

    edit_record(registry_root, "printing", damage, region="en")
    with pytest.raises(ValueError, match="face mapping"):
        plan_preview(registry_root, evidence(inputs, en=False), regions=("jp",))


def test_evidence_identity_is_not_inferred_from_lookup_key(
    registry_root: Path, inputs: Inputs
) -> None:
    provider = evidence(inputs)
    cards = dict(provider.cards)
    cards["jp", "BP02-071"] = cards["jp", "PR-001"]
    plan = plan_preview(registry_root, replace(provider, cards=cards), regions=("jp",))
    assert len(plan.included("printing")) == 1
    assert any("observation_mismatch" in p.reasons for p in plan.projections)


def test_source_face_count_has_its_own_guard(
    registry_root: Path, inputs: Inputs
) -> None:
    provider = evidence(inputs)
    cards = dict(provider.cards)
    original = cards["jp", "BP02-071"]
    cards["jp", "BP02-071"] = replace(
        original, faces=(*original.faces, FaceEvidence("LG", None))
    )
    with pytest.raises(ValueError, match="cover extracted faces"):
        plan_preview(registry_root, replace(provider, cards=cards), regions=("jp",))


def test_missing_parents_does_not_write_any_identity(
    registry_root: Path, inputs: Inputs
) -> None:
    plan = plan_preview(registry_root, evidence(inputs), regions=("jp",))
    with create_database(compile_build()) as db:
        with pytest.raises(ValueError, match="Missing product_family"):
            import_preview(db, plan, build=BUILD, authored_revision=REVISION)
        for table in ("source_record", "card", "printing"):
            assert not db.rows(table)


def test_late_failure_rolls_back_all_imported_rows(
    registry_root: Path, inputs: Inputs
) -> None:
    plan = plan_preview(registry_root, evidence(inputs), regions=("jp", "en"))
    # Deliberately omit optional art DDL so failure occurs after inserting the full audit trail.
    with create_database(compile_build()) as db:
        parents(db, plan)
        with pytest.raises(KeyError, match="art"):
            import_preview(db, plan, build=BUILD, authored_revision=REVISION)
        assert not db.rows("source_record")
        assert not db.rows("card")
        assert db.rows("product_family")


def test_source_version_conflict_is_not_silently_deduplicated(
    registry_root: Path, inputs: Inputs
) -> None:
    provider = evidence(inputs)
    cards = dict(provider.cards)
    other = cards["jp", "PR-001"]
    cards["jp", "PR-001"] = replace(
        other,
        source=other.source.model_copy(
            update={"id": cards["jp", "BP02-071"].source.id}
        ),
    )
    plan = plan_preview(registry_root, replace(provider, cards=cards), regions=("jp",))
    with create_database(compile_build()) as db:
        parents(db, plan)
        with pytest.raises(ValueError, match="Conflicting raw source metadata"):
            import_preview(db, plan, build=BUILD, authored_revision=REVISION)
        assert not db.rows("source_record")


@pytest.mark.parametrize("regions", [(), ("jp", "jp"), ("xx",)])
def test_output_regions_are_explicit(
    registry_root: Path, inputs: Inputs, regions: tuple[str, ...]
) -> None:
    with pytest.raises(ValueError, match="Explicit unique"):
        plan_preview(registry_root, evidence(inputs), regions=regions)  # type: ignore[arg-type]  # exercise invalid runtime input


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("rules_hash", "sha256:" + "f" * 64),
        ("observation_hash", "sha256:" + "e" * 64),
        ("recipe", "unsupported-recipe"),
        ("region", "en"),
        ("card_no", "BP02-071EN"),
    ],
)
def test_each_observation_component_is_checked(
    registry_root: Path,
    inputs: Inputs,
    field: str,
    value: str,
) -> None:
    provider = evidence(inputs)
    cards = dict(provider.cards)
    original = cards["jp", "BP02-071"]
    cards["jp", "BP02-071"] = replace(
        original, observation=original.observation.model_copy(update={field: value})
    )
    plan = plan_preview(registry_root, replace(provider, cards=cards), regions=("jp",))
    assert len(plan.included("printing")) == 1
    assert any("observation_mismatch" in p.reasons for p in plan.projections)


def test_jp_database_has_no_references_to_excluded_region(
    registry_root: Path, inputs: Inputs
) -> None:
    plan = plan_preview(registry_root, evidence(inputs, en=False), regions=("jp",))
    with create_database(compile_build()) as db:
        parents(db, plan)
        import_preview(db, plan, build=BUILD, authored_revision=REVISION)
        assert {r.values["region"] for r in db.rows("printing")} == {"jp"}
        assert {r.values["int_id"] for r in db.rows("card_int_id")} == {20001, 20002}
        assert all(r.values["art_id"] is None for r in db.rows("printing_face"))
        assert all(
            r.values["printed_effect_unit_id"] is None for r in db.rows("printing_face")
        )
        assert not db.rows("decision")
        db.verify()


def test_no_suffix_based_match_when_the_exact_target_source_is_missing(
    registry_root: Path, inputs: Inputs
) -> None:
    provider = evidence(inputs)
    cards = dict(provider.cards)
    del cards["jp", "BP02-071"]
    plan = plan_preview(
        registry_root, replace(provider, cards=cards), regions=("jp", "en")
    )
    assert {
        r.data.card_no
        for r in plan.included("printing")
        if isinstance(r.data, PrintingData)
    } == {"PR-001", "GF01-001EN"}
    assert not plan.included("card_related")


def test_confirmed_none_changed_own_source_is_excluded(
    registry_root: Path, inputs: Inputs
) -> None:
    inputs.en["GF01-001EN"].faces[0].text = "Changed after historic review."
    plan = plan_preview(registry_root, evidence(inputs), regions=("en",))
    item = next(
        p for p in plan.projections if p.record_key.startswith("region_mapping_review:")
    )
    assert "mapping_evidence_unavailable" in item.reasons


@pytest.mark.parametrize("bad_revision", ["", "a" * 7, "z" * 40])
def test_authored_source_requires_full_revision(
    registry_root: Path, inputs: Inputs, bad_revision: str
) -> None:
    plan = plan_preview(registry_root, evidence(inputs), regions=("jp",))
    with create_database(compile_build()) as db:
        parents(db, plan)
        with pytest.raises(ValueError, match="full Git commit"):
            import_preview(db, plan, build=BUILD, authored_revision=bad_revision)
        assert not db.rows("source_record")


def test_conflicting_art_uses_fail_atomically(
    registry_root: Path, inputs: Inputs
) -> None:
    path = next((registry_root / "registry/art").rglob("*.yaml"))
    shard = Shard.model_validate(read_yaml(path))
    original = shard.records[0]
    copy = original.model_copy(deep=True)
    copy.data["id"] = "a:" + "f" * 32
    shard.records.append(copy)
    rewrite(path, shard)
    plan = plan_preview(registry_root, evidence(inputs), regions=("en",))
    with create_database(compile_build(("en", "related"))) as db:
        parents(db, plan)
        with pytest.raises(ValueError, match="Multiple adopted art groups"):
            import_preview(db, plan, build=BUILD, authored_revision=REVISION)
        assert not db.rows("art")
        assert not db.rows("source_record")


def test_art_without_adopted_baseline_is_explicitly_deferred(
    registry_root: Path, inputs: Inputs
) -> None:
    def change(entry: Entry) -> None:
        entry.data["classification"] = "alternate"

    edit_record(registry_root, "art", change)
    plan = plan_preview(registry_root, evidence(inputs), regions=("en",))
    assert not plan.included("art")
    assert any(p.reasons == ("art_baseline_deferred",) for p in plan.projections)
