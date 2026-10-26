"""Real whole-card handoffs preserve declarations and Fanfare independently."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import array, object_value

from ...support.current_keyword_fixtures import (
    current_combat_keyword_inputs,
    current_keyword_inputs,
)

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize("number", ["BP01-138", "BP19-P32", "BP19-114"])
def test_real_keyword_handoff_contains_every_printed_sibling(
    tmp_path: Path, number: str
) -> None:
    selection = object_value(current_keyword_inputs(tmp_path, number))
    author = object_value(array(selection["authors"])[0])
    assert author["status"] == "accepted"
    handoff = object_value(author["input"])
    assert handoff["card_no"] == number
    declaration = object_value(array(handoff["declaration_sidecar"])[0])
    assert object_value(declaration["declaration"])["keywords"] == ["ward", "aura"]
    assert "ability_scope" not in object_value(declaration["source_use"])
    bodies = array(handoff["sidecar"])
    assert len(bodies) == (0 if number == "BP01-138" else 1)
    if bodies:
        inventory = object_value(handoff["source_inventory"])
        body = object_value(array(inventory["abilities"])[0])
        assert object_value(body["body"])["fanfare"] == {
            "do": [
                {
                    "destroy": {
                        "ref": {"who": "opp", "zone": "field", "kind": "follower"}
                    }
                }
            ]
        }


def test_combat_handoffs_include_all_current_printings_after_public_reconstruction(
    tmp_path: Path,
) -> None:
    expected = {
        "BP01-096": (["intimidate"], 0),
        "BP01-158": (["storm"], 0),
        "BP01-163": (["assail"], 0),
        "BP01-P10": (["storm"], 0),
        "BP03-033": (["bane"], 0),
        "BP04-P07": (["bane"], 1),
        "BP14-032": (["storm"], 0),
        "DSD01b-004": (["storm"], 1),
        "DSD01b-005": (["storm", "aura"], 0),
        "SD07-006": (["storm"], 1),
        "BP09-114": (["rush"], 1),
    }
    selections = current_combat_keyword_inputs(tmp_path)
    assert selections.keys() == expected.keys()
    for number, selection in selections.items():
        author = object_value(array(object_value(selection)["authors"])[0])
        assert author["status"] == "accepted"
        handoff = object_value(author["input"])
        declaration = object_value(array(handoff["declaration_sidecar"])[0])
        keywords, bodies = expected[number]
        assert object_value(declaration["declaration"])["keywords"] == keywords
        assert len(array(handoff["sidecar"])) == bodies
