"""Current term references preserve exact concepts without changing frozen v1 code."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.glossary.records import TermRecord
from sve_carddb.domains.translations.inputs import load_glossary
from sve_carddb.domains.translations.parameters.adopted_references import adopted

from ...support.translation_fixtures import name_term, write

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.provenance import Source
    from sve_carddb.domains.catalog.adoption_models import SourceRef


class NoSources:
    def text(self, _ref: SourceRef) -> tuple[str, str, Source]:
        pytest.fail("Synthetic authored source must not read an archive")


def test_current_reference_uses_full_current_glossary(tmp_path: Path) -> None:
    values = [
        TermRecord.model_validate_json(canonical(name_term())).model_dump(
            mode="json", round_trip=True
        )
    ]
    write(
        tmp_path,
        {
            "translations/glossary/concepts/001.yaml": {
                "format": 2,
                "kind": "translation_shard",
                "records": list[JsonValue](values),
            }
        },
    )
    references = adopted(load_glossary(tmp_path), NoSources())
    assert references.quoted("Synthetic card").target is not None
    assert references.quoted("Different synthetic card").issues == (
        "missing_card_name_concept",
    )
