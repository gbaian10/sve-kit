"""The registry-observation-v2 boundary excludes unrelated page metadata."""

from typing import TYPE_CHECKING

from sve_carddb.domains.registry.inputs import Card, Face

if TYPE_CHECKING:
    from sve_carddb.parse.pages import extract_en, extract_jp


def jp_card(record: extract_jp.CardRecord) -> Card:
    """Preserve parsed traits, wording and source face order."""
    return Card(
        number=record.number,
        faces=[
            Face(
                name=face.name,
                card_class=face.card_class,
                card_type=face.card_type,
                traits=face.traits,
                cost=face.cost,
                power=face.power,
                hp=face.hp,
                text=face.text,
                sections=face.sections,
                info={},
                stats={},
                speech=None,
                image=face.image,
            )
            for face in record.faces
        ],
    )


def en_card(record: extract_en.CardRecord) -> Card:
    """Keep parsed effect and sections separate from raw display text."""
    return Card(
        number=record.number,
        faces=[
            Face(
                name=face.name,
                card_class="",
                card_type="",
                traits=face.traits,
                cost="-",
                power="-",
                hp="-",
                text=face.text,
                sections=face.sections,
                info=face.info,
                stats=face.stats,
                speech=face.speech,
                image=face.image,
            )
            for face in record.faces
        ],
    )
