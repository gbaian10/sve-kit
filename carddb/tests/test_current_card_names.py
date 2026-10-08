"""Names can be prepared without a review event; source fields remain mandatory."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.domains.translations.inputs import load_glossary
from sve_carddb.domains.translations.names.candidates import Candidate, prepare

from .translation_fixtures import reference, write

if TYPE_CHECKING:
    from pathlib import Path


def candidate() -> Candidate:
    return Candidate.model_validate(
        {
            "concept_key": "name.synthetic",
            "source_ref": reference(provider="jp", locator="/faces/0/name"),
            "text": "合成譯名",
            "origin": "machine",
            "low_confidence": True,
            "source_claim": None,
        }
    )


def test_current_card_name_quality_and_no_receipt(tmp_path: Path) -> None:
    write(tmp_path, {})
    shards = prepare((candidate(),), load_glossary(tmp_path))
    assert len(shards) == 2
    assert "decisions" not in shards[0].model_dump()
    choice = shards[1].records[0]
    assert choice.origin == "machine"
    assert choice.low_confidence is True
    assert choice.record_key == '["glossary_choice","term:name.synthetic","zh-Hant"]'


def test_current_card_name_source_and_collision_refusals(tmp_path: Path) -> None:
    write(tmp_path, {})
    snapshot = load_glossary(tmp_path)
    with pytest.raises(ValueError, match=r"^Card-name concept keys must be unique$"):
        prepare((candidate(), candidate()), snapshot)
    ref = candidate().source_ref.model_copy(update={"locator": "/faces/0/effect"})
    with pytest.raises(
        ValueError, match=r"^Card-name concept requires a frozen Japanese name field$"
    ):
        prepare((candidate().model_copy(update={"source_ref": ref}),), snapshot)
