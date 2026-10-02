"""Exact frozen compound fields compose through derived receipts rather than caller codes."""

import copy
import re
import shutil
from dataclasses import replace
from typing import TYPE_CHECKING
from unittest.mock import patch

import pytest
from pydantic import JsonValue

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.catalog import adoption_importer
from sve_carddb.catalog.adoption_importer import derive_catalog, import_adopted_text
from sve_carddb.registry.preview.importer import populate_identity_rows
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.text_observations import text_configuration

from .adoption_display_fixtures import DisplayCase, make_display_case
from .adoption_fixtures import commit, dependency, envelope, index, record, write

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import CompiledSchema
    from sve_carddb.build_inputs import BuildContext


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_t0()


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> DisplayCase:
    case = make_display_case(
        tmp_path_factory.mktemp("exact-catalog") / "case",
        variants=False,
        raw_type="Synthetic type / Evolved",
    )
    return adopt_compound(case)


def adopt_compound(case: DisplayCase) -> DisplayCase:
    first = case.plan.observations[0]
    ref = case.name_refs[0] | {
        "locator": "/faces/0/card_type",
        "text_hash": digest(first.content.type_raw.encode()),
    }
    special = record(
        "vocabulary_adoption",
        {"kind": "special_kind", "code": "evolve"},
        {
            "label": {"kind": "authored", "lang": "ja", "text": "Synthetic evolve"},
            "raw_mappings": [],
            "active": True,
        },
        case.case.review,
        [dependency("language", code="ja")],
    )
    term = record(
        "vocabulary_adoption",
        {"kind": "type", "code": "follower"},
        {
            "label": {
                "kind": "authored",
                "lang": "ja",
                "text": "Synthetic baseline follower",
            },
            "raw_mappings": [
                {
                    "region": "jp",
                    "lang": "ja",
                    "raw": first.content.type_raw,
                    "source_ref": ref,
                    "special_kinds": ["evolve"],
                }
            ],
            "active": True,
        },
        case.case.review,
        [
            dependency("language", code="ja"),
            dependency("vocabulary", kind="special_kind", code="evolve"),
        ],
    )
    term["evidence"] = [{"source_ref": ref, "role": "Synthetic whole compound field"}]
    write(
        case.case.root,
        "catalog-adoptions/vocabulary/shared/001.yaml",
        envelope([special, term], case.case.review),
    )
    index(case.case.root)
    return replace(case, case=replace(case.case, revision=commit(case.case.repository)))


@pytest.fixture(scope="module")
def corrected_case(tmp_path_factory: pytest.TempPathFactory) -> DisplayCase:
    case = adopt_compound(
        make_display_case(
            tmp_path_factory.mktemp("catalog-correction") / "case",
            variants=False,
            raw_type="Synthetic type / Evolved",
            correction=True,
        )
    )
    shutil.rmtree(case.case.root / "catalog-adoptions/rules-names")
    index(case.case.root)
    return replace(case, case=replace(case.case, revision=commit(case.case.repository)))


@pytest.fixture
def case(tmp_path: Path, baseline: DisplayCase) -> DisplayCase:
    return _copy_case(tmp_path, baseline)


def _copy_case(tmp_path: Path, baseline: DisplayCase) -> DisplayCase:
    repository = tmp_path / "repository"
    shutil.copytree(baseline.case.repository, repository)
    return replace(
        baseline,
        case=replace(
            baseline.case,
            repository=repository,
            root=repository / "authored",
            review=copy.deepcopy(baseline.case.review),
        ),
    )


@pytest.fixture(scope="module")
def unknown_class_baseline(tmp_path_factory: pytest.TempPathFactory) -> DisplayCase:
    return adopt_compound(
        make_display_case(
            tmp_path_factory.mktemp("unknown-class") / "case",
            variants=False,
            raw_class="Synthetic unknown class",
            raw_type="Synthetic type / Evolved",
        )
    )


@pytest.fixture
def unknown_class_case(
    tmp_path: Path, unknown_class_baseline: DisplayCase
) -> DisplayCase:
    return _copy_case(tmp_path, unknown_class_baseline)


def context(case: DisplayCase, schema: CompiledSchema) -> BuildContext:
    build = case.context()
    with create_database(schema) as probe:
        case.parents(probe)
        with probe.transaction():
            populate_identity_rows(
                probe,
                case.plan.publication_identity(),
                authored_revision=case.case.revision,
                build=build,
            )
        projection = derive_catalog(
            probe,
            case.inputs(),
            build=build,
            stores={"test-store": case.archive},
            text_plan=case.plan,
        )
    configuration = object_value(
        parse(build.configuration.encode())
    ) | text_configuration(case.plan, projection.vocabulary, ())
    return build.model_copy(update={"configuration": canonical(configuration).decode()})


def test_imports_exact_compound_observations_with_adopted_baseline_label(
    case: DisplayCase, schema: CompiledSchema
) -> None:
    build = context(case, schema)
    with create_database(schema) as db:
        case.parents(db)
        result = import_adopted_text(
            db,
            case.inputs(),
            build=build,
            stores={"test-store": case.archive},
            text_plan=case.plan,
        )
        assert len(db.rows("printing_face_observation")) == 2
        assert len(db.rows("face_revision")) == 1
        assert db.rows("face_revision")[0].values["type_code"] == "follower"
        assert db.rows("face_revision")[0].values["class_code"] is None
        assert db.rows("face_special_kind")[0].values["special_kind_code"] == "evolve"
        vocabulary = next(
            row.values for row in db.rows("vocabulary") if row.values["kind"] == "type"
        )
        label = next(
            row.values
            for row in db.rows("text_unit")
            if row.values["id"] == vocabulary["label_unit_id"]
        )
        assert label["text"] == "Synthetic baseline follower"
        assert label["text"] != "Synthetic type / Evolved"
        assert result.uses
        result.verify(db, build, result.uses, complete=False)


def test_complete_text_import_preserves_an_inactive_vocabulary_value(
    case: DisplayCase, schema: CompiledSchema
) -> None:
    path = "catalog-adoptions/vocabulary/shared/001.yaml"
    members = [
        object_value(value)
        for value in array(object_value(read_yaml(case.case.root / path))["records"])
    ]
    current = next(
        item
        for item in members
        if object_value(object_value(item["data"])["subject"])["kind"] == "type"
    )
    data = object_value(current["data"])
    value = object_value(data["value"])
    inactive = record(
        "vocabulary_adoption",
        {"kind": "type", "code": "spell"},
        value
        | {
            "active": False,
            "label": {"kind": "authored", "lang": "ja", "text": "Synthetic inactive"},
        },
        case.case.review,
        array(data["dependencies"]),
    )
    inactive["evidence"] = copy.deepcopy(current["evidence"])
    members.append(inactive)
    write(case.case.root, path, envelope(list[JsonValue](members), case.case.review))
    index(case.case.root)
    case = replace(case, case=replace(case.case, revision=commit(case.case.repository)))
    build = context(case, schema)
    with create_database(schema) as db:
        case.parents(db)
        import_adopted_text(
            db,
            case.inputs(),
            build=build,
            stores={"test-store": case.archive},
            text_plan=case.plan,
        )
        row = next(r for r in db.rows("vocabulary") if r.values["code"] == "spell")
        assert row.values["active"] is False
        assert {r.values["type_code"] for r in db.rows("face_revision")} == {"follower"}


def test_combined_import_checks_the_complete_adoption_sources_once(
    case: DisplayCase, schema: CompiledSchema
) -> None:
    build = context(case, schema)
    with (
        create_database(schema) as db,
        patch.object(
            adoption_importer,
            "_prepare_adoptions",
            wraps=adoption_importer._prepare_adoptions,
        ) as prepare,
    ):
        case.parents(db)
        result = import_adopted_text(
            db,
            case.inputs(),
            build=build,
            stores={"test-store": case.archive},
            text_plan=case.plan,
        )
        assert prepare.call_count == 1
        assert len(db.rows("printing_face_observation")) == 2
        result.verify(db, build, result.uses, complete=False)


@pytest.mark.parametrize("unknown_type", [False, True])
def test_readonly_derivation_lists_all_unknown_fields_once(
    unknown_class_case: DisplayCase, schema: CompiledSchema, unknown_type: bool
) -> None:
    case = unknown_class_case
    if unknown_type:
        path = "catalog-adoptions/vocabulary/shared/001.yaml"
        members = [
            object_value(value)
            for value in array(
                object_value(read_yaml(case.case.root / path))["records"]
            )
        ]
        term = next(
            value
            for value in members
            if object_value(object_value(value["data"])["subject"])["kind"] == "type"
        )
        data = object_value(term["data"])
        object_value(data["value"])["raw_mappings"] = []
        data["dependencies"] = [dependency("language", code="ja")]
        term["evidence"] = []
        write(
            case.case.root, path, envelope(list[JsonValue](members), case.case.review)
        )
        index(case.case.root)
        case = replace(
            case, case=replace(case.case, revision=commit(case.case.repository))
        )
    missing: list[JsonValue] = [
        {
            "region": "jp",
            "kind": "class",
            "raw": "Synthetic unknown class",
            "candidates": [],
        }
    ]
    if unknown_type:
        missing.append(
            {
                "region": "jp",
                "kind": "type",
                "raw": "Synthetic type / Evolved",
                "candidates": [],
            }
        )
    message = (
        "Unadopted vocabulary spellings: "
        + canonical(missing).decode()
        + "; new spellings require maintainer confirmation"
    )
    build = case.context()
    with create_database(schema) as db:
        case.parents(db)
        with db.transaction():
            populate_identity_rows(
                db,
                case.plan.publication_identity(),
                authored_revision=case.case.revision,
                build=build,
            )
        assert len(case.plan.observations) == 2
        before = {
            table: db.rows(table)
            for table in ("printing", "source_record", "decision", "vocabulary")
        }
        with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
            derive_catalog(
                db,
                case.inputs(),
                build=build,
                stores={"test-store": case.archive},
                text_plan=case.plan,
            )
        assert before == {table: db.rows(table) for table in before}


def test_caller_cannot_substitute_a_different_vocabulary_or_drop_text_pins(
    case: DisplayCase, schema: CompiledSchema
) -> None:
    build = context(case, schema)
    configuration = object_value(parse(build.configuration.encode()))
    configuration["text_vocabulary"] = canonical({"bindings": [], "terms": []}).decode()
    build = build.model_copy(
        update={"configuration": canonical(configuration).decode()}
    )
    with create_database(schema) as db:
        case.parents(db)
        before = {
            table: db.rows(table)
            for table in ("source_record", "decision", "vocabulary", "text_unit")
        }
        with pytest.raises(
            ValueError, match=r"^Explicit text observation configuration is not pinned$"
        ):
            import_adopted_text(
                db,
                case.inputs(),
                build=build,
                stores={"test-store": case.archive},
                text_plan=case.plan,
            )
        assert not db.rows("printing")
        assert not db.rows("face_revision")
        assert before == {table: db.rows(table) for table in before}


def test_unadopted_current_compound_spelling_is_not_split_or_guessed(
    case: DisplayCase, schema: CompiledSchema
) -> None:
    # The prior receipt is a different whole spelling, despite its valid frozen field.
    path = "catalog-adoptions/vocabulary/shared/001.yaml"
    members = [
        object_value(value)
        for value in array(object_value(read_yaml(case.case.root / path))["records"])
    ]
    term = next(
        value
        for value in members
        if object_value(object_value(value["data"])["subject"])["kind"] == "type"
    )
    object_value(object_value(term["data"])["value"])["raw_mappings"] = []
    object_value(term["data"])["dependencies"] = [dependency("language", code="ja")]
    term["evidence"] = []
    write(case.case.root, path, envelope(list[JsonValue](members), case.case.review))
    index(case.case.root)
    case = replace(case, case=replace(case.case, revision=commit(case.case.repository)))
    missing: list[JsonValue] = [
        {
            "region": "jp",
            "kind": "type",
            "raw": "Synthetic type / Evolved",
            "candidates": [],
        }
    ]
    message = (
        "Unadopted vocabulary spellings: "
        + canonical(missing).decode()
        + "; new spellings require maintainer confirmation"
    )
    with create_database(schema) as db:
        case.parents(db)
        with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
            import_adopted_text(
                db,
                case.inputs(),
                build=case.context(),
                stores={"test-store": case.archive},
                text_plan=case.plan,
            )
        assert not db.rows("printing")
        assert not db.rows("vocabulary")


@pytest.mark.parametrize("forged", [False, True])
def test_formal_catalog_rechecks_complete_frozen_correction_image_metadata(
    corrected_case: DisplayCase, schema: CompiledSchema, forged: bool
) -> None:
    case = corrected_case
    assert case.plan.corrections is not None
    application = case.plan.corrections[0]
    assert application.status == "applied"
    if forged:
        image = application.images[0].model_copy(
            update={"fetched_at": "2026-10-02T00:00:00Z"}
        )
        assert image != application.images[0]
        plan = replace(case.plan, corrections=(replace(application, images=(image,)),))
        case = replace(case, plan=plan)
    build = case.context()
    with create_database(schema) as db:
        case.parents(db)
        with db.transaction():
            populate_identity_rows(
                db,
                case.plan.publication_identity(),
                authored_revision=case.case.revision,
                build=build,
            )
        if forged:
            with pytest.raises(
                ValueError, match=r"^Adoption text source-use frozen closure mismatch$"
            ):
                derive_catalog(
                    db,
                    case.inputs(),
                    build=build,
                    stores={"test-store": case.archive},
                    text_plan=case.plan,
                )
        else:
            derived = derive_catalog(
                db,
                case.inputs(),
                build=build,
                stores={"test-store": case.archive},
                text_plan=case.plan,
            )
            assert derived.vocabulary.lookup(
                "jp", "type", "Synthetic type / Evolved"
            ).special_kinds == ("evolve",)
