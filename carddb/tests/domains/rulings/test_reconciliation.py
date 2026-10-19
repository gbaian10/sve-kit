"""Candidate multiplicity is diagnostic, never proof of a ruling's historical domain."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.domains.rulings.reconciliation import reconcile

from ..effects.test_classifications import classification, member
from ..effects.test_matching import frame
from .test_evidence import frozen
from .test_rulings import ruling

if TYPE_CHECKING:
    from sve_carddb.domains.effects.matching import FrameOccurrence


@pytest.mark.parametrize(
    ("frames", "category"),
    [
        ((frame(),), "single_current_frame"),
        ((frame(), frame(revision="second")), "multiple_frames"),
        ((frame(current=False),), "historical_source"),
        ((frame(raw="different"),), "unmatched_member"),
        ((), "unmatched_member"),
    ],
)
def test_candidate_categories_preserve_each_original_ordinal_and_ir(
    frames: tuple[FrameOccurrence, ...], category: str
) -> None:
    identifier = "T0123456789"
    data = replace(
        frozen(),
        members=(member().model_copy(update={"template": identifier}),),
        classifications=(classification().model_copy(update={"id": identifier}),),
    )
    result = reconcile((ruling(),), data, frames)
    assert result["reference_count"] == 3
    assert result["template_reference_count"] == 2
    assert result["ir_reference_count"] == 1
    assert result["candidate_categories"] == {category: 2}
    resolution = result["resolution"]
    assert isinstance(resolution, dict)
    assert resolution["reference_pending"] == 2
    assert resolution["active_template_edges"] == 0
    assert resolution["mappings"] == []
    references = result["references"]
    assert isinstance(references, list)
    assert len(references) == 3
    assert references[-1] == {
        "ruling_ref": {"id": "R-0001", "revision": 1, "reference_ordinal": 2},
        "ruling_source": ruling().source.model_dump(mode="json"),
        "original": "E.synthetic",
        "kind": "ir",
        "category": "retained_non_template",
    }


def test_lost_definition_remains_in_reconciliation() -> None:
    result = reconcile((ruling(),), frozen(), (frame(),))
    assert result["candidate_categories"] == {"missing_definition": 2}
    assert result["reference_count"] == 3
