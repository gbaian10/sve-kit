"""Preserve the historical English registry observation recipe."""

from typing import TYPE_CHECKING

from sve_carddb.registry.inputs import Card

if TYPE_CHECKING:
    from sve_carddb.parse.pages.extract_en import CardRecord


def legacy_projection(record: CardRecord) -> Card:
    """Reproduce EN registry input; sections remain inside the original full text."""
    return Card.model_validate(
        {
            "number": record.number,
            "faces": [
                {
                    "name": face.name,
                    "info": face.info,
                    "stats": face.stats,
                    "text": face.raw_text,
                    "speech": face.speech,
                    "image": face.image,
                }
                for face in record.faces
            ],
        }
    )
