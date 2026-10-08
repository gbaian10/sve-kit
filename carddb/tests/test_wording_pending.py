"""Independent release, source completeness and current/pending counterexamples."""

from dataclasses import replace
from operator import itemgetter
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json, create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.contracts.snapshot import validate
from sve_carddb.products import load_products
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.text_observations import (
    import_text_observations,
    plan_text_observations,
)
from sve_carddb.text_observations.wording import (
    printing_dates,
    printing_observed_texts,
    wording_region_blocks,
    wording_views,
)

from .build_db_fixtures import rows
from .registry_snapshot_fixtures import edit_record
from .source_correction_fixtures import make_correction_case
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture
from .text_observation_fixtures import make_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database
    from sve_carddb.registry.review import Inputs
    from sve_carddb.registry.storage import Entry
    from sve_carddb.text_observations.plan import TextPlan
    from sve_carddb.text_observations.wording import WordingView


def product(
    db: Database,
    printing: str,
    day: str | None,
    precision: str = "day",
    *,
    override: str | None = None,
) -> None:
    fixtures = rows()
    identifier = "synthetic-product-" + str(len(db.rows("product")))
    with db.transaction():
        db.insert(
            "product",
            fixtures["product"]
            | {
                "id": identifier,
                "name_unit_id": db.rows("text_unit")[0].values["id"],
                "source_id": db.rows("source_record")[0].values["id"],
                "released_on": day,
                "date_precision": precision,
                "date_raw": "Synthetic incomplete date"
                if precision in {"month", "year"}
                else None,
            },
        )
        db.insert(
            "printing_product",
            fixtures["printing_product"]
            | {
                "printing_id": printing,
                "product_id": identifier,
                "first_available_on": None,
                "first_available_precision": override,
                "first_available_raw": None,
                "source_id": db.rows("source_record")[0].values["id"],
            },
        )


def pending(db: Database, plan: TextPlan) -> WordingView:
    view = next(iter(wording_views(db, plan).values()))[0]
    validate("WordingView", view.wire())
    return view


@pytest.mark.parametrize(
    "dates", [("2020-01-01", "2026-01-01"), ("2026-01-01", "2020-01-01")]
)
def test_latest_complete_product_day_is_a_display_and_never_a_current(
    tmp_path: Path, inputs: Inputs, dates: tuple[str, str]
) -> None:
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
        observations = case.plan.groups[0].observations
        for item, day in zip(observations, dates, strict=True):
            product(db, item.printing_id, day)
        view = pending(db, case.plan)
        latest = max(zip(observations, dates, strict=True), key=itemgetter(1))[0]
        own = printing_observed_texts(db, case.plan)
        assert view.display.basis == "latest_known_release"
        assert (
            view.display.revision_id
            == own[latest.printing_id, latest.face_id][0].revision_id
        )
        assert len(view.candidates) == 2
        assert not view.undated_printing_ids
        assert not db.rows("face_current")
        blocks = wording_region_blocks(db, case.plan)
        assert blocks[latest.card_id] == [
            {"region": "jp", "reasons": ["wording_pending"]}
        ]
        assert len(db.rows("printing")) == 2
        assert all(
            row.values["printed_text_state"] == "unknown"
            for row in db.rows("printing_face")
        )


@pytest.mark.parametrize(
    "condition",
    [
        "same_day",
        "latest_null",
        "no_date",
        "unknown_override",
        "month",
        "year",
        "unknown_inclusion",
        "source_unavailable",
    ],
)
def test_unavailable_or_undated_latest_is_not_silently_skipped(
    tmp_path: Path, inputs: Inputs, condition: str
) -> None:
    if condition == "latest_null":
        inputs.jp["PR-001"].faces[0].text = None
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    if condition == "source_unavailable":
        del case.provider.cards["jp", "PR-001"]
        case.plan = plan_text_observations(case.identity, case.provider)
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
        printings = {}
        for row in db.rows("printing"):
            number, identifier = row.values["card_no"], row.values["id"]
            assert isinstance(number, str)
            assert isinstance(identifier, str)
            printings[number] = identifier
        if condition != "no_date":
            product(db, printings["BP02-071"], "2020-01-01")
            if condition in {"month", "year"}:
                product(db, printings["PR-001"], None, condition)
            else:
                product(
                    db,
                    printings["PR-001"],
                    "2020-01-01" if condition == "same_day" else "2026-01-01",
                    override="unknown" if condition == "unknown_override" else None,
                )
            if condition == "unknown_inclusion":
                product(db, printings["PR-001"], None, "unknown")
        view = pending(db, case.plan)
        assert len(view.candidates) == 2
        if condition in {"same_day", "latest_null", "no_date", "source_unavailable"}:
            assert view.display.basis == "candidates"
            assert view.display.revision_id is None
        else:
            assert view.display.basis == "latest_known_release"
            assert printings["PR-001"] in view.undated_printing_ids
        if condition in {"latest_null", "source_unavailable"}:
            assert (
                next(
                    c for c in view.candidates if c.printing_id == printings["PR-001"]
                ).revision_id
                is None
            )
        if condition == "no_date":
            assert len(view.undated_printing_ids) == 2


def test_multicollection_uses_first_available_day_and_unknown_overrides_block_it(
    tmp_path: Path, inputs: Inputs
) -> None:
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
        first, second = case.plan.groups[0].observations
        product(db, first.printing_id, "2020-01-01")
        product(db, first.printing_id, "2099-01-01")
        product(db, second.printing_id, "2025-01-01")
        assert printing_dates(db)[first.printing_id] == "2020-01-01"
        assert (
            pending(db, case.plan).display.revision_id
            == printing_observed_texts(db, case.plan)[
                second.printing_id, second.face_id
            ][0].revision_id
        )
        product(db, first.printing_id, None, "unknown")
        assert printing_dates(db)[first.printing_id] is None
        assert first.printing_id in pending(db, case.plan).undated_printing_ids


def test_valid_old_current_wins_and_pending_does_not_imply_settled(
    tmp_path: Path, inputs: Inputs
) -> None:
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
        item = case.plan.groups[0].observations[0]
        revision = printing_observed_texts(db, case.plan)[
            item.printing_id, item.face_id
        ][0].revision_id
        with db.transaction():
            db.insert(
                "face_current",
                {
                    "face_id": item.face_id,
                    "region": "jp",
                    "revision_id": revision,
                    "basis": "latest_observed_no_errata",
                },
            )
        view = pending(db, case.plan)
        assert view.display.basis == "current"
        assert view.display.revision_id == revision
        assert len(view.candidates) == 2
        assert not wording_region_blocks(db, case.plan)


def test_unknown_printing_does_not_hide_known_latest_and_block_is_in_support(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    with create_database(compile_build()) as db:
        with db.transaction():
            case.stage(db)
        group = case.plan.groups[0]
        item = group.observations[0]
        support = rows()["card_engine_support"] | {
            "card_id": item.card_id,
            "region": "jp",
            "reason_codes": Json(["no_formal_candidate"]),
        }
        with db.transaction():
            db.insert("card_engine_support", support)
        import_text_observations(
            db,
            case.plan,
            build=case.context(),
            vocabulary=case.vocabulary,
            published=(),
        )
        product(db, item.printing_id, "2026-01-01")
        view = pending(db, case.plan)
        assert view.display.basis == "latest_known_release"
        assert len(view.undated_printing_ids) == 1
        assert db.rows("card_engine_support")[0].values["reason_codes"] == Json(
            ["no_formal_candidate", "wording_pending"]
        )
        assert db.rows("card_engine_support")[0].values["automatic"] is False


def test_identical_latest_is_deduplicated_and_settled_omits_wording(
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
        assert not wording_views(db, case.plan)
        group = case.plan.groups[0]
        with db.transaction():
            db.delete("face_current", {"face_id": group.face_id, "region": "jp"})
        for item in group.observations:
            product(db, item.printing_id, "2025-01-01")
        view = pending(db, case.plan)
        assert view.display.basis == "latest_known_release"
        assert len({item.revision_id for item in view.candidates}) == 1


def test_double_face_keeps_settled_front_and_unknown_back_visible(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.mapping.reskins = {}
    for collection in (inputs.jp, inputs.en):
        for card in collection.values():
            card.faces.append(card.faces[0].model_copy(deep=True))
            card.faces[1].name += " back"
    inputs.jp["PR-001"].faces[0].text = "Rule."
    inputs.jp["PR-001"].faces[1].text = None
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
        front = next(g for g in case.plan.groups if g.current() is not None)
        back = next(g for g in case.plan.groups if "missing_effect" in g.reasons)
        for item in back.observations:
            product(
                db,
                item.printing_id,
                "2026-01-01" if item.card_no == "PR-001" else "2020-01-01",
            )
        views = wording_views(db, case.plan)
        assert front.face_id not in views
        assert views[back.face_id][0].display.basis == "candidates"
        assert len(views[back.face_id][0].candidates) == 2
        own = printing_observed_texts(db, case.plan)
        assert len(own) == 4
        assert (
            sum(
                o.state == "missing_effect" for entries in own.values() for o in entries
            )
            == 1
        )
        assert len(db.rows("printing_face")) == 4
        assert len(db.rows("face")) == 2
        assert len(case.plan.publication_identity().included("printing")) == 2


def test_all_sources_unavailable_keep_pending_positions_and_source_urls(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    case.provider.cards.clear()
    case.plan = plan_text_observations(case.identity, case.provider)
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
        view = pending(db, case.plan)
        assert view.display.basis == "candidates"
        assert all(c.revision_id is None for c in view.candidates)
        own = printing_observed_texts(db, case.plan)
        assert len(own) == 2
        assert all(
            o.state == "missing_effect" and o.revision_id is None and o.source_url
            for entries in own.values()
            for o in entries
        )
        assert not db.rows("face_revision")
        assert len(db.rows("printing")) == 2


def test_correction_revision_ids_with_exact_same_content_can_share_display(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_correction_case(tmp_path, inputs).texts
    group = next(g for g in case.plan.groups if g.region == "jp")
    assert group.current() is not None
    assert len({item.content.fingerprint() for item in group.observations}) == 1
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
            db.delete("face_current", {"face_id": group.face_id, "region": "jp"})
        for item in group.observations:
            product(db, item.printing_id, "2026-01-01")
        view = next(
            v for v in wording_views(db, case.plan)[group.face_id] if v.region == "jp"
        )
        assert len({c.revision_id for c in view.candidates}) == 2
        assert view.display.revision_id == min(
            c.revision_id for c in view.candidates if c.revision_id is not None
        )
        assert view.display.basis == "latest_known_release"
        own = printing_observed_texts(db, case.plan)
        assert {c.revision_id for c in view.candidates} == {
            own[item.printing_id, item.face_id][0].revision_id
            for item in group.observations
        }


def test_conflicting_correction_retains_its_publication_integrity_gate(
    tmp_path: Path, inputs: Inputs
) -> None:
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
        assert not any(
            v.region == "jp"
            for views in wording_views(db, case.plan).values()
            for v in views
        )
        own = printing_observed_texts(db, case.plan)
        excluded = {
            item.printing_id for item in case.plan.observations if item.region == "jp"
        }
        assert not any(printing in excluded for printing, _ in own)


def test_candidate_projection_requires_materialized_revision(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    with create_database(compile_build()) as db:
        with db.transaction():
            case.stage(db)
        with pytest.raises(ValueError, match="candidate revision is missing"):
            wording_views(db, case.plan)
        with pytest.raises(ValueError, match="observation revision is missing"):
            printing_observed_texts(db, case.plan)


def test_release_projection_rejects_wrong_regional_product(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    with create_database(compile_build()) as db:
        with db.transaction():
            case.stage(db)
        printing = case.plan.groups[0].observations[0].printing_id
        product(db, printing, "2026-01-01")
        with db.transaction():
            db.update(
                "product", {"id": db.rows("product")[0].values["id"]}, {"region": "en"}
            )
        with pytest.raises(ValueError, match="product/printing region mismatch"):
            printing_dates(db)
