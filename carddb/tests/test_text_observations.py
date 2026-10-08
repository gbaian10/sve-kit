"""Independent counterexamples for raw retention, initial selection and quarantine."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.products.models import LocalizedText
from sve_carddb.registry.review import Correction
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.text_observations import (
    Binding,
    Vocabulary,
    diagnostic_exclusion_report,
    import_text_observations,
    plan_text_observations,
)
from sve_carddb.text_observations.importer import stat
from sve_carddb.text_observations.intern import TextInterner
from sve_carddb.text_observations.plan import verify_plan

from .registry_snapshot_fixtures import edit_record
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture
from .text_observation_fixtures import make_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.review import Inputs
    from sve_carddb.registry.storage import Entry

    from .shared_case_fixtures import TextCaseTemplate


def test_exact_initial_keeps_every_physical_source_and_one_revision_per_face_region(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.jp["PR-001"].faces[0].text = "Rule."
    case = make_case(tmp_path / "authored", inputs)
    assert case.plan.report()["initial_current_count"] == 3
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
        assert len(db.rows("printing_face_observation")) == 4
        assert len(db.rows("face_revision")) == 3
        assert len(db.rows("face_current")) == 3
        assert all(row.values["decision_id"] is None for row in db.rows("face_current"))
        assert all(
            row.values["effective_from"] is None
            and row.values["temporal_status"] == "unknown"
            for row in db.rows("face_revision")
        )
        assert all(
            row.values["printed_effect_unit_id"] is None
            and row.values["printed_text_state"] == "unknown"
            for row in db.rows("printing_face")
        )
        assert {
            row.values["source_id"] for row in db.rows("printing_face_observation")
        } == {item.card.source.id for item in case.plan.observations}


@pytest.mark.parametrize("missing", [None, ""])
def test_missing_effect_stays_distinct_from_present_empty(
    tmp_path: Path, inputs: Inputs, missing: str | None
) -> None:
    for card in inputs.jp.values():
        card.faces[0].text = missing
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    report = case.plan.report()
    assert report["total_observations"] == 2
    assert report["materialized_observations"] == (0 if missing is None else 2)
    assert report["deferred_missing_effect_observations"] == (
        2 if missing is None else 0
    )
    assert report["initial_current_count"] == (0 if missing is None else 1)
    assert len(case.plan.source_uses()) == 4
    possible = report["possible_no_effect_follower_observations"]
    assert isinstance(possible, list)
    assert len(possible) == (2 if missing is None else 0)
    with create_database(compile_build()) as db:
        with db.transaction():
            case.stage(db)
        result = import_text_observations(
            db,
            case.plan,
            build=case.context(),
            vocabulary=case.vocabulary,
            published=(),
        )
        assert len(result.uses) == 4
        assert len(db.rows("printing_face_observation")) == (
            0 if missing is None else 2
        )
        assert len(db.rows("face_revision")) == (0 if missing is None else 1)
        if missing is None:
            assert all(row.values["text"] for row in db.rows("text_unit"))
        else:
            assert "" in {row.values["text"] for row in db.rows("text_unit")}
        assert all(
            row.values["parser_version"] is None
            for row in db.rows("source_record")
            if row.values["kind"] != "authored"
        )


def test_sections_are_ordered_unknown_and_not_removed_from_diffs(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.jp["PR-001"].faces[0].text = "Rule."
    inputs.jp["BP02-071"].faces[0].sections = ["First.", "", "Last."]
    inputs.jp["PR-001"].faces[0].sections = ["Last.", "", "First."]
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    assert case.plan.groups[0].current() is None
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
        assert len(db.rows("face_revision")) == 2
        assert len(db.rows("face_text_section")) == 6
        assert {row.values["kind"] for row in db.rows("face_text_section")} == {
            "unknown"
        }
        assert all(
            row.values["decision_id"] is None for row in db.rows("face_text_section")
        )
        assert not db.rows("face_current")
        assert not db.rows("printing_text_section")


def test_partial_double_face_quarantines_entire_card_region_and_all_ids(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.mapping.reskins = {}
    for collection in (inputs.jp, inputs.en):
        for card in collection.values():
            card.faces.append(card.faces[0].model_copy(deep=True))
            card.faces[1].name += " back"
    inputs.jp["PR-001"].faces[0].text = "Rule."
    inputs.jp["PR-001"].faces[1].text = "Different back."
    case = make_case(tmp_path / "authored", inputs)
    schema = compile_build(("en",))
    assert any(
        group.region == "jp" and group.current() is not None
        for group in case.plan.groups
    )
    assert not any(
        item.disposition == "included" and "jp" in item.regions
        for item in case.plan.diagnostic_exclusions.projections
        if item.record_key.startswith("printing:")
    )
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
        report = diagnostic_exclusion_report(db, schema, case.plan)
        counts = report["excluded_row_counts"]
        assert isinstance(counts, dict)
        assert counts["printing"] == 2
        assert counts["card_int_id"] == 2
        assert counts["printing_face"] == 4
        assert counts["printing_face_observation"] == 4
        assert counts["face_revision"] == 3
        assert counts["face_current"] == 1
        assert counts["text_unit"] == 0


@pytest.mark.parametrize("field", ["sections", "stats", "traits", "name"])
def test_each_current_field_difference_is_independently_pending(
    tmp_path: Path, inputs: Inputs, field: str
) -> None:
    inputs.jp["PR-001"].faces[0].text = "Rule."
    case = make_case(tmp_path / "authored", inputs)
    card = case.provider.cards["jp", "PR-001"]
    changes = {
        "sections": ("Extra.",),
        "stats": ("3", "2", "1"),
        "traits": ("Synthetic trait",),
        "name": "Different name",
    }
    content = card.faces[0].model_copy(update={field: changes[field]})
    case.provider.cards["jp", "PR-001"] = card.model_copy(update={"faces": (content,)})
    plan = plan_text_observations(case.identity, case.provider)
    group = next(group for group in plan.groups if group.region == "jp")
    assert group.reasons == ("observation_difference",)
    assert group.current() is None


@pytest.mark.parametrize(
    "value", ["-1", "1.5", " 1", "１", "9007199254740992", "unknown"]
)
def test_unrepresentable_numeric_stats_are_not_invented(value: str) -> None:
    with pytest.raises(ValueError, match="Unrepresentable"):
        stat(value)


@pytest.mark.parametrize("value", ["-", "X", "Q"])
def test_unknown_and_symbolic_stats_stay_null(value: str) -> None:
    assert stat(value) is None
    assert stat("0") == 0
    assert stat("17") == 17


def test_missing_source_blocks_a_good_sibling_instead_of_selecting_it(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.jp["PR-001"].faces[0].text = "Rule."
    case = make_case(tmp_path / "authored", inputs)
    del case.provider.cards["jp", "PR-001"]
    plan = plan_text_observations(case.identity, case.provider)
    group = next(group for group in plan.groups if group.region == "jp")
    assert group.reasons == ("missing_source",)
    assert group.current() is None
    assert len(plan.unavailable) == 1
    verify_plan(plan)


def test_errata_link_keeps_even_identical_observations_pending(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.jp["PR-001"].faces[0].text = "Rule."
    case = make_case(tmp_path / "authored", inputs)
    card = case.provider.cards["jp", "PR-001"]
    case.provider.cards["jp", "PR-001"] = card.model_copy(
        update={"has_errata_link": True}
    )
    plan = plan_text_observations(case.identity, case.provider)
    group = next(group for group in plan.groups if group.region == "jp")
    assert group.reasons == ("errata_pending",)
    assert group.current() is None


@pytest.mark.parametrize("field", ["class_raw", "type_raw", "title"])
def test_each_additional_current_field_difference_is_pending(
    tmp_path: Path, inputs: Inputs, field: str
) -> None:
    inputs.jp["PR-001"].faces[0].text = "Rule."
    case = make_case(tmp_path / "authored", inputs)
    card = case.provider.cards["jp", "PR-001"]
    face = card.faces[0].model_copy(update={field: "Synthetic different"})
    case.provider.cards["jp", "PR-001"] = card.model_copy(update={"faces": (face,)})
    plan = plan_text_observations(case.identity, case.provider)
    assert (
        next(group for group in plan.groups if group.region == "jp").current() is None
    )


def test_flavor_is_physical_and_does_not_choose_a_different_current(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.jp["PR-001"].faces[0].text = "Rule."
    inputs.jp["PR-001"].faces[0].speech = "Different physical flavor"
    case = make_case(tmp_path / "authored", inputs)
    group = next(group for group in case.plan.groups if group.region == "jp")
    assert group.current() is not None
    with create_database(compile_build(("en", "related"))) as db:
        with db.transaction():
            case.stage(db)
        import_text_observations(
            db,
            case.plan,
            build=case.context(),
            vocabulary=case.vocabulary,
            published=(),
        )
        flavors = {
            row.values["flavor_unit_id"]
            for row in db.rows("printing_face")
            if row.values["printing_id"]
            in {item.printing_id for item in group.observations}
        }
        assert len(flavors) == 2


def test_reversed_source_indices_use_authored_faces_not_face_ordinals(
    tmp_path: Path, inputs: Inputs
) -> None:
    inputs.mapping.reskins = {}
    for collection in (inputs.jp, inputs.en):
        for card in collection.values():
            card.faces.append(card.faces[0].model_copy(deep=True))
            card.faces[1].name += " back"
    case = make_case(tmp_path / "authored", inputs)

    def reverse(entry: Entry) -> None:
        mappings = entry.data["source_face_map"]
        assert isinstance(mappings, list)
        assert isinstance(mappings[0], dict)
        assert isinstance(mappings[1], dict)
        mappings[0]["face_id"], mappings[1]["face_id"] = (
            mappings[1]["face_id"],
            mappings[0]["face_id"],
        )

    edit_record(case.root, "printing", reverse, region="en")
    identity = replace(case.identity, snapshot=load_registry(case.root))
    plan = plan_text_observations(identity, case.provider)
    target = next(
        o
        for o in plan.observations
        if o.region == "en" and o.source_index == 0 and o.card_no == "BP02-070EN"
    )
    face = identity.snapshot.records["face:" + target.face_id].data
    assert face.model_dump()["ordinal"] == 1
    assert target.content.name == inputs.en["BP02-070EN"].faces[0].name
    verify_plan(plan)


@pytest.mark.parametrize("state", ["active", "needs_review"])
def test_source_correction_does_not_implicitly_adopt_or_apply_text(
    tmp_path: Path, inputs: Inputs, state: str
) -> None:
    inputs.jp["PR-001"].faces[0].text = "Rule."
    inputs.decisions.corrections = [
        Correction(
            region="jp",
            card_no="BP02-071",
            field="effect",
            expected_raw_value="Rule.",
            corrected_value="Correction candidate",
            image_sha256="sha256:" + "1" * 64,
            locator="effect",
            state=state,
            reason="Synthetic correction",
        )
    ]
    case = make_case(tmp_path / "authored", inputs)
    group = next(group for group in case.plan.groups if group.region == "jp")
    assert group.reasons == ("source_correction_pending",)
    assert group.current() is None
    assert all(item.content.effect == "Rule." for item in group.observations)


def test_explicit_special_kinds_traits_and_titles_are_all_materialized(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    bindings = list(case.vocabulary.bindings)
    for number, special in (("BP02-071", "evolved"), ("PR-001", "token")):
        original = case.provider.cards["jp", number]
        content = original.faces[0].model_copy(
            update={
                "type_raw": "Synthetic " + special,
                "title": "Synthetic universe",
                "traits": ("Synthetic trait",),
            }
        )
        case.provider.cards["jp", number] = original.model_copy(
            update={"faces": (content,)}
        )
        bindings.extend(
            (
                Binding(
                    region="jp",
                    kind="type",
                    raw=content.type_raw,
                    code="follower",
                    special_kinds=(special,),
                ),
                Binding(region="jp", kind="special_kind", raw=special, code=special),
            )
        )
    bindings.extend(
        (
            Binding(
                region="jp",
                kind="title",
                raw="Synthetic universe",
                code="synthetic_title",
            ),
            Binding(
                region="jp", kind="trait", raw="Synthetic trait", code="synthetic_trait"
            ),
        )
    )
    case.vocabulary = Vocabulary(bindings=tuple(bindings))
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
        assert len(db.rows("face_title")) == 2
        assert len(db.rows("face_trait")) == 2
        assert {
            row.values["special_kind_code"] for row in db.rows("face_special_kind")
        } == {"evolved", "token"}


class TestDefaultTextInputs:
    def test_wording_difference_never_selects_latest_date_or_overwrites_source(
        self, tmp_path: Path, default_text_case: TextCaseTemplate
    ) -> None:
        case = default_text_case.copy(tmp_path)
        for key, value in tuple(case.provider.cards.items()):
            case.provider.cards[key] = value.model_copy(
                update={
                    "date_raw": "2099-12-31" if key[1] == "PR-001" else "1900-01-01"
                }
            )
        plan = plan_text_observations(case.identity, case.provider)
        jp = [group for group in plan.groups if group.region == "jp"]
        assert len(jp) == 1
        assert jp[0].reasons == ("observation_difference",)
        assert jp[0].current() is None
        assert len(jp[0].observations) == 2
        assert all(
            "jp" not in item.regions
            for item in plan.diagnostic_exclusions.projections
            if item.record_key.startswith("printing:")
            and item.disposition == "included"
        )
        report = plan.report()
        assert report["total_observations"] == 4
        assert report["materialized_observations"] == 4
        assert report["difference_face_region_count"] == 1
        assert "Rule." not in str(report)
        assert "Reminder" not in str(report)
        assert report["initial_current_count"] == 2

    @pytest.mark.parametrize(
        "problem", ["source", "index", "content", "missing", "extra", "closure"]
    )
    def test_altered_observation_or_selection_plan_fails_closed(
        self, tmp_path: Path, default_text_case: TextCaseTemplate, problem: str
    ) -> None:
        case = default_text_case.copy(tmp_path)
        item = case.plan.observations[0]
        plan = case.plan
        if problem == "source":
            card = item.card.model_copy(
                update={
                    "source": item.card.source.model_copy(update={"etag": "changed"})
                }
            )
            item = item.model_copy(update={"card": card})
        elif problem == "index":
            item = item.model_copy(update={"source_index": 1})
        elif problem == "content":
            item = item.model_copy(
                update={
                    "content": item.content.model_copy(update={"effect": "Invented"})
                }
            )
        elif problem == "missing":
            plan = replace(plan, observations=plan.observations[1:])
        elif problem == "extra":
            groups = tuple(
                replace(
                    group,
                    observations=tuple(
                        sorted(
                            (*group.observations, item),
                            key=lambda obs: (obs.card.source.id, obs.printing_id),
                        )
                    ),
                )
                if (group.face_id, group.region) == (item.face_id, item.region)
                else group
                for group in plan.groups
            )
            plan = replace(plan, observations=(*plan.observations, item), groups=groups)
        else:
            plan = replace(plan, diagnostic_exclusions=plan.identity)
        if problem in {"source", "index", "content"}:
            plan = replace(plan, observations=(item, *plan.observations[1:]))
        with pytest.raises(ValueError, match="Text"):
            verify_plan(plan)

    def test_exclusion_closes_routes_aliases_defaults_and_keeps_other_region(
        self, tmp_path: Path, default_text_case: TextCaseTemplate
    ) -> None:
        case = default_text_case.copy(tmp_path)
        schema = compile_build(("en", "related"))
        item = next(item for item in case.plan.observations if item.region == "jp")
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
                db.insert(
                    "product",
                    {
                        "id": "p:synthetic",
                        "region": "jp",
                        "family_id": None,
                        "product_code": None,
                        "name_unit_id": db.rows("product_family")[0].values[
                            "name_unit_id"
                        ],
                        "product_type": None,
                        "released_on": None,
                        "date_precision": "unknown",
                        "date_raw": None,
                        "source_id": item.card.source.id,
                    },
                )
                db.insert(
                    "printing_product",
                    {
                        "printing_id": item.printing_id,
                        "product_id": "p:synthetic",
                        "first_available_on": None,
                        "first_available_precision": None,
                        "first_available_raw": None,
                        "inclusion_kind": "other",
                        "note_unit_id": None,
                        "source_id": item.card.source.id,
                    },
                )
                assert any(
                    row.values["route_key"] == item.card_no
                    and row.values["printing_id"] == item.printing_id
                    for row in db.rows("card_route")
                )
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
            counts = diagnostic_exclusion_report(db, schema, case.plan)[
                "excluded_row_counts"
            ]
            assert isinstance(counts, dict)
            assert counts["card_route"] == 2
            assert counts["card_route_alias"] == 1
            assert counts["route_override"] == 1
            assert counts["default_printing_override"] == 1
            assert counts["card"] == 0
            assert counts["printing"] == 2
            assert counts["face_revision"] == 2
            assert counts["face_current"] == 0
            assert counts["card_related"] == 0
            assert counts["printing_product"] == 1
            assert counts["product"] == 0

    @pytest.mark.parametrize("field", ["region", "card_no", "source_id"])
    def test_staged_printing_metadata_must_match_the_independent_plan(
        self, tmp_path: Path, default_text_case: TextCaseTemplate, field: str
    ) -> None:
        case = default_text_case.copy(tmp_path)
        schema = compile_build(("en", "related"))
        item = next(item for item in case.plan.observations if item.region == "jp")
        replacement = {
            "region": "en",
            "card_no": "Wrong",
            "source_id": case.plan.observations[-1].card.source.id,
        }[field]
        with create_database(schema) as db:
            with db.transaction():
                case.stage(db)
                db.update("printing", {"id": item.printing_id}, {field: replacement})
            before = {table.name: db.rows(table.name) for table in schema.tables}
            with pytest.raises(ValueError, match="identity parent"):
                import_text_observations(
                    db,
                    case.plan,
                    build=case.context(),
                    vocabulary=case.vocabulary,
                    published=(),
                )
            assert before == {
                table.name: db.rows(table.name) for table in schema.tables
            }

    @pytest.mark.parametrize(
        "problem", ["missing", "duplicate", "special_reference", "existing", "inactive"]
    )
    def test_explicit_vocabulary_errors_roll_back_text_staging(
        self, tmp_path: Path, default_text_case: TextCaseTemplate, problem: str
    ) -> None:
        case = default_text_case.copy(tmp_path)
        bindings = case.vocabulary.bindings
        if problem == "missing":
            bindings = tuple(b for b in bindings if b.kind != "type")
        elif problem == "duplicate":
            bindings = (*bindings, bindings[0])
        elif problem == "special_reference":
            bindings = (
                bindings[0].model_copy(update={"special_kinds": ("evolved",)}),
                *bindings[1:],
            )
        case.vocabulary = Vocabulary(bindings=bindings)
        schema = compile_build(("en", "related"))
        with create_database(schema) as db:
            with db.transaction():
                case.stage(db)
                if problem in {"existing", "inactive"}:
                    db.insert(
                        "vocabulary",
                        {
                            "kind": "type",
                            "code": "follower",
                            "active": problem != "inactive",
                            "label_unit_id": TextInterner(db).intern(
                                LocalizedText(lang="ja", text="フォロワー")
                            )
                            if problem == "inactive"
                            else db.rows("product_family")[0].values["name_unit_id"],
                        },
                    )
            before = {table.name: db.rows(table.name) for table in schema.tables}
            with pytest.raises(ValueError, match="vocabulary"):
                import_text_observations(
                    db,
                    case.plan,
                    build=case.context(),
                    vocabulary=case.vocabulary,
                    published=(),
                )
            assert before == {
                table.name: db.rows(table.name) for table in schema.tables
            }

    def test_provider_metadata_disagreement_is_rejected_before_planning(
        self, tmp_path: Path, default_text_case: TextCaseTemplate
    ) -> None:
        case = default_text_case.copy(tmp_path)
        card = case.provider.cards["jp", "PR-001"]
        changed = card.source.model_copy(update={"etag": "Changed metadata"})
        case.provider.cards["jp", "PR-001"] = card.model_copy(
            update={"source": changed}
        )
        with pytest.raises(ValueError, match="independently verified"):
            plan_text_observations(case.identity, case.provider)
