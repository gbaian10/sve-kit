"""Authored references cannot select or override a locally configured archive store."""

from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.catalog.adoption_models import Batch, SourceRef
from sve_carddb.products.models import Evidence
from sve_carddb.registry.transitions.models import Batch as TransitionBatch

if TYPE_CHECKING:
    from sve_carddb.registry.records import RecordData


@pytest.mark.parametrize("model", [Batch, SourceRef, Evidence, TransitionBatch])
def test_authored_store_id_is_an_unknown_field(model: type[RecordData]) -> None:
    with pytest.raises(ValidationError) as error:
        model.model_validate(
            {"batch_id": "sha256:" + "a" * 64, "store_id": "synthetic"}
        )
    assert any(
        item["type"] == "extra_forbidden" and item["loc"] == ("store_id",)
        for item in error.value.errors(include_input=False)
    )
