"""Trusted restoration rejects schema evolution it cannot reconstruct."""

from collections.abc import Mapping
from datetime import datetime
from enum import Enum
from typing import cast

import pytest
from pydantic import BaseModel, create_model

from sve_carddb.domains.parsed_cards.trusted import construct


class FutureEnum(Enum):
    VALUE = "value"


@pytest.mark.parametrize(
    ("annotation", "value"),
    [
        (datetime, "2026-10-01T00:00:00Z"),
        (FutureEnum, "value"),
        (frozenset[str], []),
        (Mapping[str, str], {}),
        (list[datetime], []),
        (datetime | None, None),
        (tuple[str, int], ["value", 1]),
        (dict[int, str], {}),
    ],
)
def test_unknown_annotations_fail_even_without_values(
    annotation: object, value: object
) -> None:
    model = create_model("FutureInput", value=(annotation, ...))
    with pytest.raises(TypeError, match="Unsupported trusted input annotation"):
        construct(model, {"value": value})


def test_trusted_restoration_does_not_run_field_validation() -> None:
    class KnownInput(BaseModel):
        value: int

    assert (
        cast("object", construct(KnownInput, {"value": "admitted"}).value) == "admitted"
    )
