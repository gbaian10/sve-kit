"""Official labels require exact adopted source fields and retain their provenance."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import array, object_value

from ...support.catalog_adoption_fixtures import populate_case
from ...support.catalog_vocabulary_fixtures import mapping, records, save
from .test_catalog_vocabulary import baseline as baseline  # ruff: ignore[useless-import-alias] -- register the immutable synthetic source fixture
from .test_catalog_vocabulary import case as case  # ruff: ignore[useless-import-alias] -- edit a separate authored repository for each case

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build import CompiledSchema

    from ...support.catalog_vocabulary_fixtures import VocabularyCase


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_build(("t0", "translation_names"))


def with_label(case: VocabularyCase, kind: str = "class") -> list[dict[str, JsonValue]]:
    items = records(case)
    data = next(
        object_value(item["data"])
        for item in items
        if object_value(object_value(item["data"])["subject"])["kind"] == kind
    )
    value = object_value(data["value"])
    raw = mapping(case, region="en", kind=kind)
    value["raw_mappings"] = [raw]
    value["translations"] = [
        {
            "lang": "en",
            "text": raw["raw"],
            "origin": "official",
            "source_ref": raw["source_ref"],
        },
        {"lang": "zh-Hant", "text": "合成標籤"},
    ]
    return items


@pytest.mark.parametrize("kind", ["class", "type"])
def test_exact_base_label_retains_official_source(
    case: VocabularyCase, schema: CompiledSchema, kind: str
) -> None:
    case = save(case, with_label(case, kind))
    with create_database(schema) as db:
        result = populate_case(db, case.case, {"test-store": case.archive})
        translations = [row.values for row in db.rows("translation")]
        translation = next(row for row in translations if row["target_lang"] == "en")
        assert translation["text"] == "Synthetic " + kind
        assert (translation["origin"], translation["authority"]) == (
            "official",
            "sve_official",
        )
        assert translation["source_id"] in {use.source.id for use in result.uses}
        assert translation["id"] in {
            row.values["translation_id"] for row in db.rows("translation_selection")
        }
        assert (
            next(row for row in translations if row["target_lang"] == "zh-Hant")["text"]
            == "合成標籤"
        )
        assert len({row["context_id"] for row in translations}) == 1
        assert len(db.rows("translation_use")) == 1


@pytest.mark.parametrize("mutation", ["text", "lang", "other_ref", "marked_type"])
def test_official_label_cannot_borrow_another_mapping_or_part_of_composite_type(
    case: VocabularyCase, schema: CompiledSchema, mutation: str
) -> None:
    items = with_label(case, "type")
    value = next(
        object_value(object_value(item["data"])["value"])
        for item in items
        if object_value(object_value(item["data"])["subject"])["kind"] == "type"
    )
    label = object_value(array(value["translations"])[0])
    if mutation == "text":
        label["text"] = "Invented label"
    elif mutation == "lang":
        label["lang"] = "zh-Hant"
    elif mutation == "other_ref":
        label["source_ref"] = case.references["en", "type", 2]
    else:
        object_value(array(value["raw_mappings"])[0])["special_kinds"] = ["evolve"]
    case = save(case, items)
    with create_database(schema) as db:
        with pytest.raises(
            ValueError, match="Official label requires an exact adopted base mapping"
        ):
            populate_case(db, case.case, {"test-store": case.archive})
        assert not db.rows("source_record")
        assert not db.rows("vocabulary")
