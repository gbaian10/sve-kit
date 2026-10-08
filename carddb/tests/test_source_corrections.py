"""Independent counterexamples for correction provenance, application and reference scope."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import Json, create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.products import load_products
from sve_carddb.registry.preview import plan_preview
from sve_carddb.registry.records import PrintingData
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.source_corrections.closure import correction_exclusions
from sve_carddb.source_corrections.importer import verify_corrections
from sve_carddb.source_corrections.plan import plan_applications
from sve_carddb.source_corrections.projection import correction_references
from sve_carddb.text_observations import (
    import_text_observations,
    plan_text_observations,
)
from sve_carddb.text_observations.importer import revision_id
from sve_carddb.text_observations.plan import verify_plan

from .identity_evidence_fixtures import MemoryEvidence
from .registry_snapshot_fixtures import edit_record
from .source_correction_fixtures import make_correction_case
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture
from .text_observation_fixtures import make_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_inputs import Source
    from sve_carddb.registry.records import CorrectionEvidence, Region
    from sve_carddb.registry.review import Inputs
    from sve_carddb.registry.storage import Entry
    from sve_carddb.source_corrections.plan import Application

    from .shared_case_fixtures import CorrectionCaseTemplate


def test_conflict_is_the_only_pending_reason_and_prevents_mechanical_current(
    tmp_path: Path, inputs: Inputs
) -> None:
    for card in inputs.jp.values():
        card.faces[0].text = "Rule."
    fixture = make_correction_case(tmp_path, inputs)
    case = fixture.texts

    def edit(entry: Entry) -> None:
        entry.data["expected_source_hash"] = "sha256:" + "0" * 64

    edit_record(case.root, "source_correction", edit)
    case.identity = replace(case.identity, snapshot=load_registry(case.root))
    case.catalog = load_products(case.root, registry=case.identity.snapshot)
    case.plan = plan_text_observations(
        case.identity, case.provider, images=fixture.images
    )
    assert case.plan.corrections is not None
    assert case.plan.corrections[0].status == "conflict"
    group = next(group for group in case.plan.groups if group.region == "jp")
    assert len({item.content.fingerprint() for item in group.observations}) == 1
    assert group.reasons == ("source_correction_pending",)
    assert group.current() is None
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
        assert not any(row.values["region"] == "jp" for row in db.rows("face_current"))


@pytest.mark.parametrize(
    ("region", "field"), [("jp", "effect"), ("en", "effect"), ("en", "card_type")]
)
def test_application_keeps_raw_revision_and_only_marks_affected_uses(  # ruff: ignore[too-many-statements] -- validates independent SQL, source, current and public reference boundaries in one transaction
    tmp_path: Path, inputs: Inputs, region: Region, field: str
) -> None:
    fixture = make_correction_case(tmp_path, inputs, region=region, field=field)
    case = fixture.texts
    assert case.plan.corrections is not None
    application = case.plan.corrections[0]
    assert application.status == "applied"
    raw = application.observation
    candidate = next(
        c for c in case.plan.candidates() if c.printing_id == raw.printing_id
    )
    assert raw.content.effect == "Rule."
    assert candidate.content != raw.content
    assert revision_id(raw) != revision_id(candidate)
    assert candidate.correction_keys == (
        digest(
            canonical(
                {
                    "record_key": "source_correction:" + application.data.id,
                    "kind": "source_correction",
                    "owner": application.record.owner,
                    "data": application.data.model_dump(mode="json"),
                }
            )
        ),
    )
    assert application.key() == candidate.correction_keys[0]
    for original, projected in zip(
        case.plan.observations, case.plan.candidates(), strict=True
    ):
        if (original.printing_id, original.face_id) != (raw.printing_id, raw.face_id):
            assert projected == original
    if region == "jp":
        assert (
            next(group for group in case.plan.groups if group.region == "jp").current()
            is not None
        )
    schema = compile_build(("en", "related", "correction"))
    with create_database(schema) as db:
        with db.transaction():
            case.stage(db)
        record = import_text_observations(
            db,
            case.plan,
            build=case.context(),
            vocabulary=case.vocabulary,
            published=(),
        )
        assert len(db.rows("source_correction")) == 1
        correction = db.rows("source_correction")[0].values
        assert correction["expected_raw_value"] == Json(
            "Rule." if field == "effect" else "Spell"
        )
        assert (
            correction["expected_source_hash"] == raw.card.observation.observation_hash
        )
        evidence = db.rows("correction_evidence")[0].values
        assert evidence["kind"] == "card_image"
        assert evidence["locator"] == "Synthetic field box"
        source = next(
            row.values
            for row in db.rows("source_record")
            if row.values["id"] == evidence["source_id"]
        )
        assert source["sha256"] == application.data.evidence[0].sha256
        assert source["kind"] == "image"
        physical = next(
            row.values
            for row in db.rows("printing_face_observation")
            if row.values["printing_id"] == raw.printing_id
        )
        assert physical["revision_id"] == revision_id(raw)
        result = db.rows("correction_application")[0].values
        assert result["status"] == "applied"
        assert result["face_revision_id"] == revision_id(candidate)
        units = {row.values["id"]: row.values["text"] for row in db.rows("text_unit")}
        revisions = {row.values["id"]: row.values for row in db.rows("face_revision")}
        assert units[revisions[revision_id(raw)]["effect_unit_id"]] == "Rule."
        assert revisions[revision_id(candidate)]["change_kind"] == "source_correction"
        assert revisions[revision_id(candidate)]["supersedes_id"] == revision_id(raw)
        assert revisions[revision_id(candidate)]["decision_id"] is None
        if field == "effect":
            assert units[result["result_unit_id"]] == "Rule. (Reminder.)"
        else:
            assert result["result_unit_id"] is None
            assert revisions[revision_id(raw)]["type_code"] == "spell"
            assert revisions[revision_id(candidate)]["type_code"] == "follower"
        references = correction_references(db, case.plan, case.vocabulary)
        assert references == (
            {
                "printing_id": raw.printing_id,
                "face_id": raw.face_id,
                "source_id": raw.card.source.id,
                "revision_id": revision_id(candidate),
                "corrections": [
                    {
                        "field": field,
                        "corrected_from": "Rule." if field == "effect" else "Spell",
                        "is_corrected": True,
                        "reason": "Synthetic source transcription correction",
                        "source_url": raw.card.source.url,
                    }
                ],
            },
        )
        assert all("corrections" not in row.values for row in db.rows("text_unit"))
        assert all(
            row.values["printed_text_state"] == "unknown"
            and row.values["printed_effect_unit_id"] is None
            for row in db.rows("printing_face")
        )
        assert (
            sum(use.usage == "source_correction_evidence" for use in record.uses) == 1
        )
        assert (
            sum(use.usage == "source_correction_comparison" for use in record.uses) == 1
        )
        assert "Rule." not in canonical(case.plan.report()).decode()


def test_proposed_correction_is_retained_but_never_applied_or_marked(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_correction_case(tmp_path, inputs, state="needs_review").texts
    assert case.plan.corrections is not None
    assert case.plan.corrections[0].status is None
    assert case.plan.candidates() == case.plan.observations
    assert (
        next(group for group in case.plan.groups if group.region == "jp").current()
        is None
    )
    assert case.plan.publication_identity() == case.identity
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
        assert db.rows("source_correction")[0].values["state"] == "needs_review"
        assert len(db.rows("correction_evidence")) == 1
        assert not db.rows("correction_application")
        assert correction_references(db, case.plan, case.vocabulary) == ()


def test_already_fixed_is_not_reapplied_even_when_old_hash_differs(
    tmp_path: Path, inputs: Inputs
) -> None:
    fixture = make_correction_case(tmp_path, inputs, corrected="Rule.")
    case = fixture.texts

    def edit(entry: Entry) -> None:
        entry.data["expected_source_hash"] = "sha256:" + "0" * 64
        entry.data["expected_raw_value"] = "Old value"

    edit_record(case.root, "source_correction", edit)
    case.identity = replace(case.identity, snapshot=load_registry(case.root))
    case.catalog = load_products(case.root, registry=case.identity.snapshot)
    case.plan = plan_text_observations(
        case.identity, case.provider, images=fixture.images
    )
    assert case.plan.corrections is not None
    assert case.plan.corrections[0].status == "already_fixed"
    assert case.plan.candidates() == case.plan.observations
    reports = case.plan.report()["corrections"]
    assert isinstance(reports, list)
    assert isinstance(reports[0], dict)
    assert reports[0]["warning"] == "retire_upstream_fixed_correction"
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
        assert db.rows("source_correction")[0].values["state"] == "upstream_fixed"
        assert db.rows("correction_application")[0].values["status"] == "already_fixed"
        assert not any(
            row.values["change_kind"] == "source_correction"
            for row in db.rows("face_revision")
        )
        assert correction_references(db, case.plan, case.vocabulary) == ()


def test_changed_source_excludes_correction_parent_without_allowing_sibling_output(
    tmp_path: Path, inputs: Inputs
) -> None:
    original = make_correction_case(tmp_path / "original", inputs)
    inputs.jp["BP02-071"].faces[0].text = "Changed synthetic source"
    changed = make_case(tmp_path / "changed", inputs)
    identity = plan_preview(
        original.texts.root,
        MemoryEvidence(
            dict(changed.identity.evidence),
            frozenset({inputs.jp_hash}),
        ),
        regions=("jp", "en"),
    )
    included = [record.data for record in identity.included("printing")]
    assert all(isinstance(data, PrintingData) for data in included)
    assert not any(
        isinstance(data, PrintingData) and data.card_no == "BP02-071"
        for data in included
    )
    assert any(
        isinstance(data, PrintingData) and data.region == "jp" for data in included
    )
    plan = plan_text_observations(identity, changed.provider, images=original.images)
    assert plan.corrections == ()
    output = plan.publication_identity()
    assert output.included("printing")
    assert {record.record_key for record in output.included("printing")} == {
        record.record_key
        for record in identity.included("printing")
        if isinstance(record.data, PrintingData) and record.data.region == "en"
    }
    assert all(
        isinstance(record.data, PrintingData) and record.data.region == "en"
        for record in output.included("printing")
    )


def test_pending_wording_remains_in_publication_scope(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_correction_case(
        tmp_path, inputs, corrected="New synthetic effect"
    ).texts
    assert (
        next(group for group in case.plan.groups if group.region == "jp").current()
        is None
    )
    assert case.plan.publication_identity() == case.identity
    assert len(case.plan.publication_identity().included("printing")) == 4
    assert case.plan.diagnostic_exclusions != case.identity


@pytest.mark.parametrize("conflict", [False, True])
def test_conflict_closure_removes_routes_aliases_and_defaults_but_keeps_pending_wording(
    tmp_path: Path, inputs: Inputs, conflict: bool
) -> None:
    fixture = make_correction_case(tmp_path, inputs, corrected="New synthetic effect")
    case = fixture.texts
    if conflict:

        def edit(entry: Entry) -> None:
            entry.data["expected_raw_value"] = "Wrong old value"

        edit_record(case.root, "source_correction", edit)
        case.identity = replace(case.identity, snapshot=load_registry(case.root))
        case.catalog = load_products(case.root, registry=case.identity.snapshot)
        case.plan = plan_text_observations(
            case.identity, case.provider, images=fixture.images
        )
    assert case.plan.corrections is not None
    item = case.plan.corrections[0].observation
    schema = compile_build(("en", "related", "correction"))
    with create_database(schema) as db:
        with db.transaction():
            case.stage(db)
            decision = "route-decision"
            db.insert(
                "decision",
                {
                    "id": decision,
                    "state": "confirmed",
                    "scope": "record",
                    "category": "route",
                    "note": "",
                },
            )
            assert any(
                row.values["namespace"] == "official"
                and row.values["route_key"] == item.card_no
                and row.values["printing_id"] == item.printing_id
                for row in db.rows("card_route")
            )
            assert len(db.rows("card_route")) == 4
            db.insert(
                "card_route_alias",
                {
                    "namespace": "official",
                    "old_key": "SYN-old",
                    "target_namespace": "official",
                    "target_key": item.card_no,
                    "reason": "renumbered",
                    "source_id": item.card.source.id,
                    "decision_id": decision,
                },
            )
            db.insert(
                "default_printing_override",
                {
                    "card_id": item.card_id,
                    "region": "jp",
                    "printing_id": item.printing_id,
                    "decision_id": decision,
                },
            )
            db.insert(
                "route_override",
                {
                    "route_key": item.card_no,
                    "printing_id": item.printing_id,
                    "decision_id": decision,
                    "namespace": "official",
                },
            )
        import_text_observations(
            db,
            case.plan,
            build=case.context(),
            vocabulary=case.vocabulary,
            published=(),
        )
        report = correction_exclusions(db, schema, case.plan)
        counts = report["excluded_row_counts"]
        assert isinstance(counts, dict)
        for table in (
            "card_route_alias",
            "default_printing_override",
            "route_override",
        ):
            assert counts[table] == (1 if conflict else 0)
        assert counts["card_route"] == (2 if conflict else 0)
        assert counts["printing"] == (2 if conflict else 0)
        assert counts["card"] == 0
        assert counts["face_current"] == 0


class TestDefaultCorrectionInputs:
    def test_domain_verifier_rejects_physical_observation_pointing_to_corrected_revision(
        self, tmp_path: Path, default_correction_case: CorrectionCaseTemplate
    ) -> None:
        case = default_correction_case.copy(tmp_path).texts
        assert case.plan.corrections is not None
        raw = case.plan.corrections[0].observation
        candidate = next(
            item
            for item in case.plan.candidates()
            if item.printing_id == raw.printing_id
        )
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
            with db.transaction():
                db.update(
                    "printing_face_observation",
                    {
                        "printing_id": raw.printing_id,
                        "face_id": raw.face_id,
                        "source_id": raw.card.source.id,
                    },
                    {"revision_id": revision_id(candidate)},
                )
            with pytest.raises(ValueError, match="original physical observation"):
                verify_corrections(db, case.plan, case.vocabulary)

    def test_other_image_provider_cannot_accept_cross_region_evidence(
        self, tmp_path: Path, default_correction_case: CorrectionCaseTemplate
    ) -> None:
        fixture = default_correction_case.copy(tmp_path)
        case = fixture.texts
        assert case.plan.corrections is not None
        source = case.plan.corrections[0].images[0]

        def edit(entry: Entry) -> None:
            evidence = entry.data["evidence"]
            assert isinstance(evidence, list)
            assert isinstance(evidence[0], dict)
            evidence[0]["region"] = "en"
            evidence[0]["image_src"] = source.url

        edit_record(case.root, "source_correction", edit)
        identity = replace(case.identity, snapshot=load_registry(case.root))

        class FixedImages:
            def image(self, _evidence: CorrectionEvidence) -> Source:
                return source

        with pytest.raises(
            ValueError, match="Correction image evidence metadata mismatch"
        ):
            plan_applications(identity, case.plan.observations, FixedImages())

    def test_selected_correction_requires_raw_observation_at_planning_boundary(
        self, tmp_path: Path, default_correction_case: CorrectionCaseTemplate
    ) -> None:
        fixture = default_correction_case.copy(tmp_path)
        case = fixture.texts
        assert case.plan.corrections is not None
        target = case.plan.corrections[0].observation
        remaining = tuple(item for item in case.plan.observations if item != target)
        with pytest.raises(
            ValueError, match="Correction requires its raw face observation"
        ):
            plan_applications(case.identity, remaining, fixture.images)

    @pytest.mark.parametrize("field", ["expected_raw_value", "expected_source_hash"])
    def test_each_conflict_pin_independently_blocks_only_affected_region(
        self,
        tmp_path: Path,
        default_correction_case: CorrectionCaseTemplate,
        field: str,
    ) -> None:
        fixture = default_correction_case.copy(tmp_path)
        case = fixture.texts

        def edit(entry: Entry) -> None:
            entry.data[field] = (
                "Different exact value"
                if field == "expected_raw_value"
                else "sha256:" + "0" * 64
            )

        edit_record(case.root, "source_correction", edit)
        case.identity = replace(case.identity, snapshot=load_registry(case.root))
        case.catalog = load_products(case.root, registry=case.identity.snapshot)
        case.plan = plan_text_observations(
            case.identity, case.provider, images=fixture.images
        )
        assert case.plan.corrections is not None
        assert case.plan.corrections[0].status == "conflict"
        assert case.plan.candidates() == case.plan.observations
        output = case.plan.publication_identity()
        for record in output.included("printing"):
            assert isinstance(record.data, PrintingData)
            assert record.data.region == "en"
        assert len(output.included("card_related")) == 1
        assert (
            next(group for group in case.plan.groups if group.region == "jp").current()
            is None
        )
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
            row = db.rows("correction_application")[0].values
            assert row["status"] == "conflict"
            assert row["result_unit_id"] is None
            assert row["face_revision_id"] is None
            assert db.rows("source_correction")[0].values["state"] == "needs_review"
            assert correction_references(db, case.plan, case.vocabulary) == ()

    @pytest.mark.parametrize(
        "problem",
        [
            "omitted",
            "duplicate",
            "status",
            "source",
            "kind",
            "url",
            "hash",
            "region",
        ],
    )
    def test_each_altered_application_or_evidence_is_rejected_before_writes(
        self,
        tmp_path: Path,
        default_correction_case: CorrectionCaseTemplate,
        problem: str,
    ) -> None:
        case = default_correction_case.copy(tmp_path).texts
        assert case.plan.corrections is not None
        application = case.plan.corrections[0]
        applications: tuple[Application, ...]
        if problem == "omitted":
            applications = ()
        elif problem == "duplicate":
            applications = (application, application)
        elif problem == "status":
            applications = (replace(application, status="already_fixed"),)
        elif problem == "source":
            applications = (
                replace(application, observation=case.plan.observations[-1]),
            )
        elif problem in {"kind", "url", "hash"}:
            changed = application.images[0].model_copy(
                update={
                    "kind"
                    if problem == "kind"
                    else "url"
                    if problem == "url"
                    else "sha256": "official_page"
                    if problem == "kind"
                    else "https://example.invalid/wrong"
                    if problem == "url"
                    else "sha256:" + "0" * 64
                }
            )
            applications = (replace(application, images=(changed,)),)
        elif problem == "region":
            data = application.data.model_copy(
                update={
                    "evidence": (
                        application.data.evidence[0].model_copy(
                            update={"region": "en"}
                        ),
                    )
                }
            )
            applications = (
                replace(application, record=replace(application.record, data=data)),
            )
        changed_plan = replace(case.plan, corrections=applications)
        with pytest.raises(ValueError, match="Correction"):
            verify_plan(changed_plan)

    @pytest.mark.parametrize("field", ["reason", "corrected_value", "state"])
    def test_correction_change_invalidates_old_candidate_and_configuration(
        self,
        tmp_path: Path,
        default_correction_case: CorrectionCaseTemplate,
        field: str,
    ) -> None:
        fixture = default_correction_case.copy(tmp_path)
        case = fixture.texts
        old = case.plan
        old_configuration = case.context()

        def edit(entry: Entry) -> None:
            entry.data[field] = (
                "needs_review" if field == "state" else "New exact " + field
            )

        edit_record(case.root, "source_correction", edit)
        case.identity = replace(case.identity, snapshot=load_registry(case.root))
        case.catalog = load_products(case.root, registry=case.identity.snapshot)
        case.plan = plan_text_observations(
            case.identity, case.provider, images=fixture.images
        )
        assert case.plan.configuration() != old.configuration()
        with pytest.raises(ValueError, match="Correction"):
            verify_plan(replace(old, identity=case.identity))
        with create_database(compile_build(("en", "related", "correction"))) as db:
            with db.transaction():
                case.stage(db)
            with pytest.raises(ValueError, match="configuration"):
                import_text_observations(
                    db,
                    case.plan,
                    build=old_configuration,
                    vocabulary=case.vocabulary,
                    published=(),
                )
            assert not db.rows("source_correction")

    @pytest.mark.parametrize(
        "field",
        [
            "result_unit_id",
            "face_revision_id",
            "expected_raw_value",
            "corrected_value",
            "locator",
            "effect_unit_id",
            "type_code",
        ],
    )
    def test_domain_verifier_rejects_structurally_valid_wrong_database_values(
        self,
        tmp_path: Path,
        default_correction_case: CorrectionCaseTemplate,
        field: str,
    ) -> None:
        case = default_correction_case.copy(tmp_path).texts
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
            row = db.rows("correction_application")[0].values
            assert case.plan.corrections is not None
            raw_id = revision_id(case.plan.corrections[0].observation)
            raw_revision = next(
                r.values for r in db.rows("face_revision") if r.values["id"] == raw_id
            )
            with pytest.raises(ValueError, match="Correction"), db.transaction():  # ruff: ignore[pytest-raises-with-multiple-statements] -- fault injection must remain inside the rolled-back transaction
                if field in {"result_unit_id", "face_revision_id"}:
                    db.update(
                        "correction_application",
                        {
                            "correction_id": row["correction_id"],
                            "source_id": row["source_id"],
                        },
                        {
                            field: raw_revision["effect_unit_id"]
                            if field == "result_unit_id"
                            else raw_id
                        },
                    )
                elif field in {"expected_raw_value", "corrected_value"}:
                    db.update(
                        "source_correction",
                        {"id": row["correction_id"]},
                        {field: Json("Wrong but structurally valid")},
                    )
                elif field == "locator":
                    evidence = db.rows("correction_evidence")[0].values
                    db.update(
                        "correction_evidence",
                        {
                            key: evidence[key]
                            for key in ("correction_id", "source_id", "kind")
                        },
                        {field: "Wrong locator"},
                    )
                elif field == "effect_unit_id":
                    db.update(
                        "face_revision",
                        {"id": row["face_revision_id"]},
                        {field: raw_revision["effect_unit_id"]},
                    )
                else:
                    db.update(
                        "face_revision",
                        {"id": row["face_revision_id"]},
                        {field: "Wrong code"},
                    )
                verify_corrections(db, case.plan, case.vocabulary)

    def test_missing_image_pin_cannot_bypass_known_corrections(
        self, tmp_path: Path, default_correction_case: CorrectionCaseTemplate
    ) -> None:
        case = default_correction_case.copy(tmp_path).texts
        plan = plan_text_observations(case.identity, case.provider)
        with pytest.raises(ValueError, match="pinned image"):
            plan.publication_identity()
        with create_database(compile_build(("en", "related", "correction"))) as db:
            with db.transaction():
                case.stage(db)
            with pytest.raises(ValueError, match="pinned image"):
                import_text_observations(
                    db,
                    plan,
                    build=case.context(),
                    vocabulary=case.vocabulary,
                    published=(),
                )
            assert not db.rows("face_revision")

    def test_public_source_url_null_is_present_and_never_substitutes_image_url(
        self, tmp_path: Path, default_correction_case: CorrectionCaseTemplate
    ) -> None:
        case = default_correction_case.copy(tmp_path).texts
        assert case.plan.corrections is not None
        assert case.plan.corrections[0].marker(None) == {
            "field": "effect",
            "corrected_from": "Rule.",
            "is_corrected": True,
            "reason": "Synthetic source transcription correction",
            "source_url": None,
        }
