"""Anchored independent counterexamples for native current compound mappings."""

import copy
import re
import shutil
from dataclasses import replace
from typing import TYPE_CHECKING, cast

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.current import compile_current_build
from sve_carddb.catalog.adoption_models import RawMapping
from sve_carddb.catalog.adoption_validation import _mapping_metadata
from sve_carddb.snapshot.values import canonical, object_value
from sve_carddb.text_observations.vocabulary import Vocabulary

from .catalog_vocabulary_fixtures import (
    VocabularyCase,
    make_vocabulary_case,
    mapping,
    records,
    save,
    vocabulary_record,
)
from .current_catalog_fixtures import current_case, populate_case, prepare_case
from .test_glossary_adoption import checked

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import CompiledSchema


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_current_build()


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> VocabularyCase:
    return make_vocabulary_case(tmp_path_factory.mktemp("compound-catalog"))


@pytest.fixture
def case(tmp_path: Path, baseline: VocabularyCase) -> VocabularyCase:
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
        references=copy.deepcopy(baseline.references),
    )


def test_native_current_catalog_has_bilingual_bindings_and_empty_marker_definitions(
    case: VocabularyCase,
    schema: CompiledSchema,
) -> None:
    case = replace(case, case=current_case(case.case))
    with create_database(schema) as db:
        derived = prepare_case(case.case, {"test-store": case.archive}).projection
        assert not db.rows("vocabulary")
        assert not db.rows("source_record")
        assert len(derived.catalog.terms) == 3
        assert not [b for b in derived.vocabulary.bindings if b.kind == "special_kind"]
        for region in ("jp", "en"):
            binding = derived.vocabulary.lookup(region, "type", "Synthetic type")
            assert (binding.code, binding.special_kinds) == ("follower", ("evolve",))
        result = populate_case(db, case.case, {"test-store": case.archive})
        assert len(db.rows("vocabulary")) == 3
        assert {u.source.id for u in result.uses}


def test_inactive_type_retains_its_key_but_cannot_bind_a_frozen_spelling(
    case: VocabularyCase,
) -> None:
    case = save(
        case,
        [
            vocabulary_record(case, "type", "follower", []),
            vocabulary_record(case, "type", "spell", [mapping(case)], active=False),
        ],
    )
    derived = prepare_case(case.case, {"test-store": case.archive}).projection
    inactive = next(t for t in derived.catalog.terms if t.code == "spell")
    assert not inactive.active
    assert not derived.vocabulary.bindings
    message = "Missing or ambiguous explicit vocabulary binding: kind='type', region='jp', raw='Synthetic type', candidates=[]; new spellings require maintainer confirmation"
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        derived.vocabulary.lookup("jp", "type", "Synthetic type")


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("missing_markers", "Invalid adoption fields"),
        ("duplicate_markers", "Adopted special kinds must be sorted and unique"),
        ("unsorted_markers", "Adopted special kinds must be sorted and unique"),
        ("unknown_marker", "Unsupported adopted type special kind"),
        ("non_type_markers", "Only type raw mappings can declare special kinds"),
        ("unknown_definition", "Unsupported adopted special-kind code"),
        ("definition_mapping", "Special-kind definitions cannot declare raw mappings"),
        ("missing_class_code", "Missing class value cannot be adopted as a code"),
        ("missing_type_code", "Missing type value cannot be adopted as a code"),
        (
            "same_code_different_markers",
            "Duplicate vocabulary binding",
        ),
        (
            "same_raw_other_code",
            "Duplicate vocabulary binding",
        ),
        ("duplicate_raw_different_ref", "Duplicate vocabulary binding"),
        (
            "missing_marker_definition",
            "Special-kind vocabulary reference is missing",
        ),
        ("inactive_marker", "Special-kind vocabulary reference is missing"),
    ],
)
def test_compound_adoption_single_rejection(  # ruff: ignore[complex-structure,too-many-branches] -- each mutation rebuilds all unrelated signed hashes and dependencies
    case: VocabularyCase,
    schema: CompiledSchema,
    mutation: str,
    message: str,
) -> None:
    items = records(case)
    base = next(
        r
        for r in items
        if object_value(object_value(r["data"])["subject"])["kind"] == "type"
    )
    if mutation in {
        "missing_marker_definition",
        "inactive_marker",
        "unknown_definition",
        "definition_mapping",
    }:
        marker = next(
            r
            for r in items
            if object_value(object_value(r["data"])["subject"])["kind"]
            == "special_kind"
        )
        if mutation == "missing_marker_definition":
            items.remove(marker)
        elif mutation == "inactive_marker":
            object_value(object_value(marker["data"])["value"])["active"] = False
        elif mutation == "unknown_definition":
            items.append(vocabulary_record(case, "special_kind", "advanced", []))
        else:
            object_value(object_value(marker["data"])["value"])["raw_mappings"] = [
                mapping(case)
            ]
            marker["evidence"] = [
                {
                    "source_ref": mapping(case)["source_ref"],
                    "role": "Synthetic complete field",
                }
            ]
    elif mutation == "same_raw_other_code":
        items.append(
            vocabulary_record(
                case, "type", "spell", [mapping(case, markers=("evolve",))]
            )
        )
    else:
        kind = (
            "class"
            if mutation in {"non_type_markers", "missing_class_code"}
            else "type"
        )
        code = "elf" if kind == "class" else "follower"
        base = next(
            r
            for r in items
            if object_value(object_value(r["data"])["subject"])
            == {"kind": kind, "code": code}
        )
        mappings = [mapping(case, kind=kind, markers=("evolve",))]
        if mutation == "missing_markers":
            mappings[0].pop("special_kinds")
        elif mutation == "duplicate_markers":
            mappings[0]["special_kinds"] = ["evolve", "evolve"]
        elif mutation == "unsorted_markers":
            mappings[0]["special_kinds"] = ["token", "evolve"]
        elif mutation == "unknown_marker":
            mappings[0]["special_kinds"] = ["advanced"]
        elif mutation.startswith("missing_"):
            mappings = [mapping(case, kind=kind, face=1)]
        elif mutation in {"same_code_different_markers", "duplicate_raw_different_ref"}:
            mappings.append(
                mapping(
                    case,
                    face=2,
                    markers=()
                    if mutation == "same_code_different_markers"
                    else ("evolve",),
                )
            )
        if mutation == "missing_markers":
            replacement = vocabulary_record(
                case, kind, code, [mapping(case, kind=kind)]
            )
            object_value(object_value(replacement["data"])["value"])["raw_mappings"] = (
                list(mappings)
            )
        else:
            replacement = vocabulary_record(case, kind, code, mappings)
        items[items.index(base)] = replacement
    case = save(case, items)
    with create_database(schema) as db:
        with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
            populate_case(db, case.case, {"test-store": case.archive})
        assert not db.rows("source_record")
        assert not db.rows("vocabulary")


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("term_duplicate", "Duplicate derived vocabulary term"),
        ("inactive_owner", "Derived binding lacks an active vocabulary term"),
        ("missing_marker", "Special-kind vocabulary reference is missing"),
    ],
)
def test_memory_vocabulary_checks_actual_term_definitions(
    case: VocabularyCase, mutation: str, message: str
) -> None:
    vocabulary = prepare_case(
        case.case, {"test-store": case.archive}
    ).projection.vocabulary
    terms = vocabulary.terms
    if mutation == "term_duplicate":
        terms = (*terms, terms[0])
    elif mutation == "inactive_owner":
        terms = tuple(
            t.model_copy(update={"active": False}) if t.kind == "type" else t
            for t in terms
        )
    else:
        terms = tuple(t for t in terms if t.kind != "special_kind")
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        Vocabulary(bindings=vocabulary.bindings, terms=terms).verify()


def test_new_spelling_lists_exact_missing_value_and_unapproved_candidates(
    case: VocabularyCase,
) -> None:
    vocabulary = prepare_case(
        case.case, {"test-store": case.archive}
    ).projection.vocabulary
    message = "Missing or ambiguous explicit vocabulary binding: kind='type', region='jp', raw='New spelling', candidates=['follower']; new spellings require maintainer confirmation"
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        vocabulary.lookup("jp", "type", "New spelling")


def test_translation_enable_flag_is_a_boolean(case: VocabularyCase) -> None:
    with pytest.raises(
        ValueError, match=r"^Translation composition flag must be boolean$"
    ):
        replace(
            case.case.inputs(), include_translations=cast("bool", "yes")
        ).translation_inputs()


def test_raw_mapping_requires_marker_field_before_membership_hashing(
    case: VocabularyCase,
) -> None:
    value = mapping(case)
    value.pop("special_kinds")
    with pytest.raises(ValueError, match=r"^Field required$"):
        checked(RawMapping, value)


def test_unknown_marker_metadata_is_independently_rejected_before_dependency_resolution(
    case: VocabularyCase,
) -> None:
    value = RawMapping.model_validate_json(
        canonical(mapping(case, markers=("advanced",)))
    )
    with pytest.raises(ValueError, match=r"^Unsupported adopted type special kind$"):
        _mapping_metadata("type", value)
