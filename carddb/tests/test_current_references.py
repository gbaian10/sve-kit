"""Current term references preserve exact concepts without changing frozen v1 code."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.snapshot.values import canonical
from sve_carddb.template_parameters.current_references import adopted
from sve_carddb.translations.current_models import TermRecord

from .translation_fixtures import name_term, write

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_inputs import Source
    from sve_carddb.catalog.adoption_models import SourceRef


class NoSources:
    def text(self, _ref: SourceRef) -> tuple[str, str, Source]:
        pytest.fail("Synthetic authored source must not read an archive")


def test_current_reference_uses_full_current_glossary(tmp_path: Path) -> None:
    values = [
        TermRecord.model_validate_json(canonical(name_term())).model_dump(mode="json")
    ]
    write(
        tmp_path,
        {
            "translations/glossary/concepts/001.yaml": {
                "translation_authored_format": 2,
                "kind": "translation_shard",
                "records": list[JsonValue](values),
            }
        },
    )
    references = adopted(tmp_path, NoSources())
    assert references.quoted("Synthetic card").target is not None
    assert references.quoted("Different synthetic card").issues == (
        "missing_card_name_concept",
    )
