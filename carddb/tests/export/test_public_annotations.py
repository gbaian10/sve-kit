"""Independent fixed cases reach the actual public annotation reader component."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import array, object_value, string
from sve_carddb.export.reader_annotations import validate_annotations

from ..support.public_annotation_fixtures import CASES, decode, inputs, view

if TYPE_CHECKING:
    from pydantic import JsonValue

CASES_TO_READ = tuple(
    object_value(c)
    for c in array(CASES["cases"])
    if object_value(c)["operation"] == "read_projection"
    and object_value(c)["group"] != "PA-01"
)


@pytest.mark.parametrize(
    "case", CASES_TO_READ, ids=lambda c: string(c["group"]) + "/" + string(c["id"])
)
def test_fixed_public_projection_component(case: dict[str, JsonValue]) -> None:
    expected = object_value(case["expected"])
    for scenario in inputs(case):
        value = object_value(scenario)
        projection = view(value)
        languages = tuple(string(v) for v in array(value["languages"]))
        if expected["result"] == "reject":
            with pytest.raises(
                ValueError, match="public-annotation/" + string(expected["reason"])
            ):
                validate_annotations(projection, languages)
        else:
            validate_annotations(projection, languages)
            selection = decode(
                "FieldTranslation",
                object_value(array(value["field_translations"])[-1])["value"],
            )
            source = object_value(selection["source"])
            assert (
                object_value(source["owner"])["kind"] == expected["source_owner_kind"]
            )
            translation = next(
                row
                for row in projection["translation"]
                if row["id"] == selection["translation_id"]
            )
            assert translation["source_unit_id"] == expected["source_text_unit_id"]
            assert translation["text_unit_id"] == expected["target_text_unit_id"]
