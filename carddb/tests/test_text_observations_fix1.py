"""Independent synthetic regressions for pending-proposal labels and review mutants."""

from dataclasses import replace
from types import MappingProxyType
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.registry.records import AllocationData, PrintingData, RelatedData
from sve_carddb.text_observations import (
    exclusion_report,
    import_text_observations,
    importer,
    plan_text_observations,
    populate_text_preview,
)
from sve_carddb.text_observations.exclusions import _close

from .registry_preview_fixtures import REVISION
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture
from .text_observation_fixtures import LANGUAGES, make_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Value
    from sve_carddb.registry.review import Inputs


def test_pending_exclusion_proposal_is_labeled_and_never_filters_staging(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_case(tmp_path / "authored", inputs)
    diagnostic = {
        "proposal": "pending-#143",
        "publication_gate": False,
        "snapshot_output_authorized": False,
    }
    report = case.plan.report()
    assert report["eligible_identity"] == case.plan.eligible.report()
    assert report["eligible_identity_diagnostic"] == diagnostic
    assert any(group.reasons for group in case.plan.groups)
    schema = compile_build(("en", "related"))
    with create_database(schema) as db, db.transaction():
        populate_text_preview(
            db,
            case.catalog,
            case.plan,
            authored_revision=REVISION,
            build=case.context(),
            vocabulary=case.vocabulary,
            published=(),
            languages=LANGUAGES,
            stores={"test-store": case.store},
        )
        assert len(db.rows("printing")) == 4
        assert len(db.rows("face_revision")) == 4
        assert len(db.rows("printing_face_observation")) == 4
        closure = exclusion_report(db, schema, case.plan)
        assert {key: closure[key] for key in diagnostic} == diagnostic


def test_initial_current_basis_never_claims_authored_adoption(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.jp["PR-001"].faces[0].text = "Rule."
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    with create_database(compile_build()) as db:
        with db.transaction():
            case.stage(db)
        import_text_observations(
            db,
            case.plan,
            build=case.context(),
            vocabulary=case.vocabulary,
            published=(),
        )
        currents = db.rows("face_current")
        assert len(currents) == 1
        assert currents[0].values["basis"] == "latest_observed_no_errata"
        assert currents[0].values["decision_id"] is None


def test_excluded_identity_variant_keeps_identical_group_pending(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.jp["PR-001"].faces[0].text = "Rule."
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    excluded = next(
        record
        for record in case.identity.included("printing")
        if isinstance(record.data, PrintingData) and record.data.card_no == "PR-001"
    )
    identity = replace(
        case.identity,
        projections=tuple(
            replace(
                item,
                disposition="excluded",
                regions=(),
                reasons=("synthetic_identity_pending",),
            )
            if item.record_key == excluded.record_key
            else item
            for item in case.identity.projections
        ),
    )
    plan = plan_text_observations(identity, case.provider)
    assert len(plan.groups) == 1
    group = plan.groups[0]
    assert len(group.observations) == 2
    assert len({item.content.fingerprint() for item in group.observations}) == 1
    assert group.reasons == ("identity_pending",)
    assert group.current() is None


@pytest.mark.parametrize("blocked", ["BP02-070EN", "GF01-001EN"])
def test_related_edge_requires_both_regional_endpoints(
    tmp_path: Path, inputs: Inputs, blocked: str
) -> None:
    inputs.en[blocked].faces[0].text = None
    case = make_case(tmp_path / "authored", inputs)
    related = case.identity.included("card_related")
    assert len(related) == 1
    data = related[0].data
    assert isinstance(data, RelatedData)
    surviving = {
        item.data.card_id
        for item in case.plan.eligible.included("printing")
        if isinstance(item.data, PrintingData) and item.data.region == "en"
    }
    assert (
        sum(
            identifier in surviving
            for identifier in (data.from_card_id, data.to_card_id)
        )
        == 1
    )
    assert not case.plan.eligible.included("card_related")


def test_allocation_projection_follows_excluded_printing_without_db_fk_help(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_case(tmp_path / "authored", inputs)
    jp_printings = {
        item.data.id
        for item in case.identity.included("printing")
        if isinstance(item.data, PrintingData) and item.data.region == "jp"
    }
    assert len(jp_printings) == 2
    projections = {item.record_key: item for item in case.plan.eligible.projections}
    allocations = [
        item
        for item in case.identity.included("card_int_id")
        if isinstance(item.data, AllocationData)
        and item.data.printing_id in jp_printings
    ]
    assert len(allocations) == 2
    for allocation in allocations:
        projected = projections[allocation.record_key]
        assert projected.disposition == "excluded"
        assert projected.regions == ()
    assert len(case.plan.eligible.included("card_int_id")) == 2


def test_reverse_fk_closure_reaches_alias_through_route_in_adverse_order(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_case(tmp_path / "authored", inputs)
    item = case.plan.observations[0]
    schema = compile_build(("en", "related"))
    with create_database(schema) as db:
        with db.transaction():
            case.stage(db)

            db.insert(
                "card_route_alias",
                {
                    "namespace": "official",
                    "old_key": "SYN-old",
                    "target_namespace": "official",
                    "target_key": item.card_no,
                    "reason": "renumbered",
                    "source_id": item.card.source.id,
                    "decision_id": db.rows("decision")[0].values["id"],
                },
            )
        # Children before parents force a second pass rather than masking a one-pass bug.
        tables = {
            name: next(table for table in schema.tables if table.name == name)
            for name in ("card_route_alias", "card_route", "printing")
        }
        rows = {name: db.rows(name) for name in tables}
        excluded: dict[str, set[tuple[Value, ...]]] = {
            "card_route_alias": set(),
            "card_route": set(),
            "printing": {(item.printing_id,)},
        }
        _close(tables, rows, excluded)
        assert excluded["card_route"] == {("official", item.card_no)}
        assert excluded["card_route_alias"] == {("official", "SYN-old")}


def test_nonfresh_text_graph_fails_before_any_new_source_write(
    tmp_path: Path, inputs: Inputs, monkeypatch: pytest.MonkeyPatch
) -> None:
    case = make_case(tmp_path / "authored", inputs)
    schema = compile_build(("en", "related"))
    with create_database(schema) as db:
        with db.transaction():
            case.stage(db)
        import_text_observations(
            db,
            case.plan,
            build=case.context(),
            vocabulary=case.vocabulary,
            published=(),
        )
        before = {table.name: db.rows(table.name) for table in schema.tables}

        def forbidden(*_args: object, **_kwargs: object) -> None:
            raise AssertionError("Nonfresh staging must fail before writing sources")

        monkeypatch.setattr(importer, "insert_raw_sources", forbidden)
        with pytest.raises(ValueError, match="fresh text staging graph"):
            import_text_observations(
                db,
                case.plan,
                build=case.context(),
                vocabulary=case.vocabulary,
                published=(),
            )
        assert before == {table.name: db.rows(table.name) for table in schema.tables}


def test_incomplete_source_face_map_fails_directly_during_planning(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.mapping.reskins = {}
    for collection in (inputs.jp, inputs.en):
        for card in collection.values():
            card.faces.append(card.faces[0].model_copy(deep=True))
            card.faces[1].name += " back"
    case = make_case(tmp_path / "authored", inputs)
    records = dict(case.identity.snapshot.records)
    original = next(
        record for record in records.values() if isinstance(record.data, PrintingData)
    )
    assert isinstance(original.data, PrintingData)
    records[original.record_key] = replace(
        original,
        data=original.data.model_copy(
            update={"source_face_map": original.data.source_face_map[:1]}
        ),
    )
    identity = replace(
        case.identity,
        snapshot=replace(case.identity.snapshot, records=MappingProxyType(records)),
    )
    with pytest.raises(ValueError, match="Text source face map is incomplete"):
        plan_text_observations(identity, case.provider)
