"""Current build media inputs remain separate from their reserved public wire shape."""

from copy import deepcopy

import pytest
from jsonschema import ValidationError
from pydantic import JsonValue

from sve_carddb.snapshot.project.shape import source_tuple


@pytest.fixture(scope="module")
def inputs() -> dict[str, dict[str, JsonValue]]:
    return {
        "printing_image": {
            "printing_id": "p:synthetic",
            "face_id": "f:synthetic",
            "image_id": "i:synthetic",
        },
        "image_variant": {
            "image_id": "i:synthetic",
            "size_key": "card_s",
            "format": "webp",
            "path": "images/sha256/aa/" + "aa" * 32 + ".webp",
            "width": 80,
            "height": 112,
            "bytes": 100,
        },
    }


@pytest.mark.parametrize("table", ["printing_image", "image_variant"])
def test_source_media_shape_is_valid(
    inputs: dict[str, dict[str, JsonValue]], table: str
) -> None:
    source_tuple(table, inputs[table])


@pytest.mark.parametrize(
    ("table", "field", "value"),
    [
        ("printing_image", "card_version", 1),
        ("printing_image", "face_id", None),
        ("printing_image", "image_id", False),
        ("image_variant", "card_version", 1),
        ("image_variant", "path", "../source.webp"),
        ("image_variant", "path", None),
        ("image_variant", "width", True),
        ("image_variant", "bytes", -1),
    ],
)
def test_source_media_rejects_invalid_shape(
    inputs: dict[str, dict[str, JsonValue]], table: str, field: str, value: JsonValue
) -> None:
    record = deepcopy(inputs[table])
    record[field] = value
    with pytest.raises((ValueError, ValidationError)):
        source_tuple(table, record)


@pytest.mark.parametrize("table", ["printing_image", "image_variant"])
def test_source_media_requires_every_field(
    inputs: dict[str, dict[str, JsonValue]], table: str
) -> None:
    for field in inputs[table]:
        record = deepcopy(inputs[table])
        del record[field]
        with pytest.raises(ValueError, match="whitelist mismatch"):
            source_tuple(table, record)
