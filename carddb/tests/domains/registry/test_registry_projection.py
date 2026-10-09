from dataclasses import replace

import pytest
from pydantic import ValidationError

from sve_carddb.domains.registry.inputs import Card, Face
from sve_carddb.domains.registry.projection import en_card, jp_card
from sve_carddb.domains.registry.records import Observation
from sve_carddb.domains.registry.review import observation
from sve_carddb.parse.pages.extract_en import extract_card as extract_en
from sve_carddb.parse.pages.extract_jp import extract_card as extract_jp

from ...parse.test_jp_compound_traits import page as jp_page
from ...support.en_extract_fixtures import page as en_page


@pytest.mark.parametrize("region", ["jp", "en"])
def test_observations_exclude_page_metadata_and_keep_source_face_order(
    region: str,
) -> None:
    if region == "jp":
        japanese = extract_jp(jp_page("Alpha・Beta"), number="SYN-001")
        card = jp_card(japanese)
        projected = jp_card(
            replace(
                japanese,
                release_date="Different date",
                qa=[],
                products=[],
                related_cards=[],
                notes=["Unrelated notice"],
            )
        )
    else:
        english = extract_en(en_page(double=True), number="SYNⓈ-01aEN")
        card = en_card(english)
        projected = en_card(
            replace(
                english,
                release_date="Different date",
                qa=[],
                products=[],
                related_cards=[],
                notes=["Unrelated notice"],
            )
        )
    assert card == projected
    assert [face.name for face in card.faces] == ["Synthetic front", "Synthetic back"]
    swapped = card.model_copy(update={"faces": list(reversed(card.faces))})
    assert observation(card, region) != observation(swapped, region)
    assert card.structure_hash(region) != swapped.structure_hash(region)


def test_grouping_uses_parsed_traits_as_a_sorted_collection() -> None:
    face = Face(
        name="Synthetic",
        image="/synthetic.png",
        traits=["Beta", "Alpha"],
        info={
            "Class": "Synthetic class",
            "Card Type": "Synthetic type",
            "Trait": "Unrelated raw trait",
        },
        stats={"cost": "1", "power": "2", "hp": "3"},
    )
    card = Card(number="SYN-001EN", faces=[face])
    reordered = Card(
        number=card.number,
        faces=[face.model_copy(update={"traits": ["Alpha", "Beta"]})],
    )
    assert face.structure("en")["traits"] == ["Alpha", "Beta"]
    assert card.structure_hash("en") == reordered.structure_hash("en")
    assert observation(card, "en") != observation(reordered, "en")
    assert face.traits == ["Beta", "Alpha"]


def test_rules_hash_only_pins_name_effect_speech_and_sections() -> None:
    face = Face(
        name="Synthetic",
        image="/synthetic.png",
        text="Effect",
        sections=["Extra"],
        speech="Speech",
    )
    card = Card(number="SYN-001", faces=[face])
    changed = Card(
        number="Different number",
        faces=[
            face.model_copy(
                update={
                    "cost": "2",
                    "image": "/other.png",
                    "traits": ["Other"],
                    "card_class": "Other",
                    "card_type": "Other",
                }
            )
        ],
    )
    assert card.rules_hash() == changed.rules_hash()
    for field, value in (
        ("name", "Other"),
        ("text", "Other"),
        ("speech", "Other"),
        ("sections", ["Other"]),
    ):
        assert (
            Card(
                number=card.number, faces=[face.model_copy(update={field: value})]
            ).rules_hash()
            != card.rules_hash()
        )


def test_reader_rejects_old_observation_recipe() -> None:
    card = Card(
        number="SYN-001", faces=[Face(name="Synthetic", image="/synthetic.png")]
    )
    actual = observation(card, "jp")
    assert Observation.model_validate(actual).recipe == "registry-observation-v2"
    with pytest.raises(ValidationError):
        Observation.model_validate(actual | {"recipe": "registry-observation-v1"})
