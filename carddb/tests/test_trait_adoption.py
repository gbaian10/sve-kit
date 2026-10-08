"""Independent complete-component counterexamples over sealed synthetic recipes."""

import copy
import re
import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.catalog.adoption_models import RawMapping, ReviewContext, SourceRef
from sve_carddb.catalog.adoption_sources import AdoptionSources
from sve_carddb.catalog.adoption_validation import term as validate_term
from sve_carddb.catalog.current_models import VocabularyRecord
from sve_carddb.core.json import canonical, digest, object_value

from .catalog_vocabulary_fixtures import save, vocabulary_record
from .current_catalog_fixtures import prepare_case
from .trait_adoption_fixtures import trait_baseline as trait_baseline  # ruff: ignore[useless-import-alias] -- register the shared immutable archive fixture

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.build_db import CompiledSchema
    from sve_carddb.catalog.projection import CatalogProjection
    from sve_carddb.registry.records import Region

    from .trait_adoption_fixtures import TraitCase

FIELD = "Vocabulary mapping source field does not match kind"
RECONSTRUCT = "Trait source components cannot reconstruct the exact raw field"
BRACKETS = "Trait source component has incomplete enclosing brackets"
SEPARATOR = "Trait source component contains an unprotected separator"


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_build()


@pytest.fixture
def case(tmp_path: Path, trait_baseline: TraitCase) -> TraitCase:
    repository = tmp_path / "repository"
    shutil.copytree(trait_baseline.vocabulary.case.repository, repository)
    base = trait_baseline.vocabulary
    return replace(
        trait_baseline,
        vocabulary=replace(
            base,
            case=replace(
                base.case,
                repository=repository,
                root=repository / "authored",
                review=copy.deepcopy(base.case.review),
            ),
        ),
    )


def derive(
    case: TraitCase, schema: CompiledSchema, records: list[dict[str, JsonValue]]
) -> CatalogProjection:
    prepared = save(case.vocabulary, records)
    with create_database(schema) as db:
        result = prepare_case(
            prepared.case, {"test-store": prepared.archive}
        ).projection
        assert not db.rows("vocabulary")
        assert not db.rows("source_record")
        return result


def term(
    mapping: dict[str, JsonValue],
    kind: str = "trait",
    code: str = "synthetic_compound",
) -> dict[str, JsonValue]:
    return vocabulary_record(kind, code, [mapping])


def rejects(
    case: TraitCase,
    schema: CompiledSchema,
    mapping: dict[str, JsonValue],
    message: str,
    kind: str = "trait",
) -> None:
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        derive(case, schema, [term(mapping, kind)])


def test_complete_protected_component_and_two_exact_english_spellings(
    case: TraitCase, schema: CompiledSchema
) -> None:
    mappings = [case.mapping(), case.mapping("en", 0, 0), case.mapping("en", 1, 0)]
    record = vocabulary_record("trait", "synthetic_compound", mappings)
    result = derive(case, schema, [record])
    for mapping in mappings:
        typed = RawMapping.model_validate_json(canonical(mapping))
        assert (
            result.vocabulary.lookup(typed.region, "trait", typed.raw).code
            == "synthetic_compound"
        )
    assert len(result.vocabulary.bindings) == 3


@pytest.mark.parametrize("region", ["jp", "en"])
@pytest.mark.parametrize(
    "locator",
    [
        "/faces/0/trait_raw",
        "/faces/0/traits",
        "/faces/0/traits/01",
        "/faces/00/traits/1",
        "/faces/0/traits/1/extra",
        "/faces/0/title",
        "/faces/0/card_class",
        "/faces/0/info/Universe",
        "/faces/0/info/Class",
        "/faces/0/traits/-1",
        "/faces/0/traits/١",
        "/faces/١/traits/1",
        "/faces/0/traits/+1",
        "/faces/0/traits/1/",
        "faces/0/traits/1",
    ],
)
def test_trait_locator_is_an_exact_ascii_component_path(
    trait_baseline: TraitCase, region: str, locator: str
) -> None:
    case = trait_baseline
    mapping = case.mapping(region, 0, 0)
    object_value(mapping["source_ref"])["locator"] = locator
    if locator == "/faces/0/trait_raw":
        raw = case.faces[region][0]["trait_raw"]
        assert isinstance(raw, str)
        mapping["raw"] = raw
        object_value(mapping["source_ref"])["text_hash"] = digest(raw.encode())
    sources = AdoptionSources(
        {"test-store": case.vocabulary.archive},
        case.vocabulary.case.repository,
    )
    record = VocabularyRecord.model_validate_json(canonical(term(mapping)))
    review = ReviewContext.model_validate_json(canonical(case.vocabulary.case.review))
    # Exercise this layer before a source guard can mask its own field check.
    with pytest.raises(ValueError, match="^" + re.escape(FIELD) + "$"):
        validate_term(record, review, sources)


@pytest.mark.parametrize("region", ["jp", "en"])
@pytest.mark.parametrize("kind", ["title", "class"])
def test_other_kinds_cannot_borrow_trait_components(
    case: TraitCase, schema: CompiledSchema, region: str, kind: str
) -> None:
    rejects(case, schema, case.mapping(region, 0, 0), FIELD, kind)


@pytest.mark.parametrize(
    ("face", "component", "message"),
    [
        (1, 0, RECONSTRUCT),
        (2, 1, BRACKETS),
        (2, 0, BRACKETS),
        (3, 1, BRACKETS),
        (4, 0, BRACKETS),
        (5, 0, BRACKETS),
        (6, 0, BRACKETS),
        (7, 0, BRACKETS),
        (8, 0, BRACKETS),
        (9, 0, SEPARATOR),
        (10, 0, RECONSTRUCT),
        (11, 0, RECONSTRUCT),
        (12, 0, RECONSTRUCT),
        (13, 0, BRACKETS),
        (14, 0, BRACKETS),
        (15, 0, RECONSTRUCT),
    ],
)
def test_even_hash_valid_source_components_must_be_complete(
    case: TraitCase, schema: CompiledSchema, face: int, component: int, message: str
) -> None:
    rejects(case, schema, case.mapping("jp", face, component), message)


@pytest.mark.parametrize(
    ("face", "message"), [(2, SEPARATOR), (3, RECONSTRUCT), (4, SEPARATOR)]
)
def test_english_trait_uses_exact_region_separator(
    case: TraitCase, schema: CompiledSchema, face: int, message: str
) -> None:
    rejects(case, schema, case.mapping("en", face, 0), message)


def test_half_component_cannot_supply_its_own_text_hash(
    case: TraitCase, schema: CompiledSchema
) -> None:
    mapping = case.mapping()
    mapping["raw"] = "〈Synthetic"
    object_value(mapping["source_ref"])["text_hash"] = digest(
        str(mapping["raw"]).encode()
    )
    rejects(case, schema, mapping, "Source locator/exact text hash mismatch")


def test_correct_text_hash_cannot_authorize_half_component_raw(
    case: TraitCase, schema: CompiledSchema
) -> None:
    mapping = case.mapping()
    mapping["raw"] = "〈Synthetic"
    rejects(
        case, schema, mapping, "Vocabulary mapping exact raw/region/language mismatch"
    )


def test_trait_does_not_gain_type_special_markers(
    case: TraitCase, schema: CompiledSchema
) -> None:
    mapping = case.mapping()
    mapping["special_kinds"] = ["evolve"]
    rejects(case, schema, mapping, "Only type raw mappings can declare special kinds")


def test_same_exact_trait_cannot_get_two_codes(
    case: TraitCase, schema: CompiledSchema
) -> None:
    records = [term(case.mapping(), code=code) for code in ("first", "second")]
    with pytest.raises(
        ValueError,
        match=r"^Duplicate vocabulary binding$",
    ):
        derive(case, schema, records)


def test_unadopted_apostrophe_is_not_an_implicit_alias(
    case: TraitCase, schema: CompiledSchema
) -> None:
    result = derive(
        case, schema, [term(case.mapping("en", 0, 0), code="synthetic_nest")]
    )
    message = (
        "Missing or ambiguous explicit vocabulary binding: kind='trait', region='en', "
        "raw='Synthetic’s Nest', candidates=['synthetic_nest']; "
        "new spellings require maintainer confirmation"
    )
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        result.vocabulary.lookup("en", "trait", "Synthetic’s Nest")


@pytest.mark.parametrize("region", ["jp", "en"])
def test_title_still_binds_its_entire_native_field(
    case: TraitCase, schema: CompiledSchema, region: Region
) -> None:
    mapping = case.mapping(region, 0, 0)
    mapping["raw"] = "Synthetic title"
    reference = object_value(mapping["source_ref"])
    reference["locator"] = "/faces/0/" + (
        "title" if region == "jp" else "info/Universe"
    )
    reference["text_hash"] = digest(b"Synthetic title")
    result = derive(case, schema, [term(mapping, "title", "synthetic_title")])
    assert (
        result.vocabulary.lookup(region, "title", "Synthetic title").code
        == "synthetic_title"
    )


@pytest.mark.parametrize(
    ("projection", "message"),
    [
        (None, "Trait source projection lacks the mapped face"),
        ({"faces": None}, "Trait source projection lacks the mapped face"),
        ({"faces": []}, "Trait source projection lacks the mapped face"),
        ({"faces": [None]}, "Trait source projection lacks the mapped face"),
        ({"faces": [{"traits": {}, "trait_raw": "Synthetic"}]}, RECONSTRUCT),
        (
            {"faces": [{"traits": {"Synthetic": "ignored"}, "trait_raw": "Synthetic"}]},
            RECONSTRUCT,
        ),
        ({"faces": [{"traits": [], "trait_raw": ""}]}, RECONSTRUCT),
    ],
)
def test_trait_projection_shape_is_checked_independently_of_the_text_resolver(
    trait_baseline: TraitCase,
    monkeypatch: pytest.MonkeyPatch,
    projection: JsonValue,
    message: str,
) -> None:
    case = trait_baseline
    mapping = case.mapping()
    sources = AdoptionSources(
        {"test-store": case.vocabulary.archive},
        case.vocabulary.case.repository,
    )
    record = VocabularyRecord.model_validate_json(canonical(term(mapping)))
    review = ReviewContext.model_validate_json(canonical(case.vocabulary.case.review))
    ref = SourceRef.model_validate_json(canonical(mapping["source_ref"]))
    text, source, _ = sources.text(ref, review)
    monkeypatch.setattr(sources, "text", lambda *_: (text, source, projection))
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        validate_term(record, review, sources)


@pytest.mark.parametrize("mutation", ["region", "language"])
def test_trait_keeps_the_existing_exact_source_guards(
    case: TraitCase, schema: CompiledSchema, mutation: str
) -> None:
    mapping = case.mapping()
    if mutation == "region":
        mapping["region"] = "en"
    elif mutation == "language":
        mapping["lang"] = "en"
    record = term(mapping)
    message = "Vocabulary mapping exact raw/region/language mismatch"
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        derive(case, schema, [record])
