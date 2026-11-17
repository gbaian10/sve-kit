"""Trusted restoration rejects schema evolution it cannot reconstruct."""

from datetime import datetime
from typing import cast

import pytest
from pydantic import BaseModel, create_model
from typing_extensions import TypeForm

from sve_carddb.domains.parsed_cards.trusted import construct


@pytest.mark.parametrize(
    ("annotation", "value"),
    [(datetime, "2026-10-01T00:00:00Z"), (datetime | None, None), (dict[int, str], {})],
)
def test_unknown_annotations_fail_even_without_values(
    annotation: TypeForm[object], value: object
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
