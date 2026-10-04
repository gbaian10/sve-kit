"""JP identity projection preserves the established registry observation boundary."""

import re
from dataclasses import asdict
from typing import TYPE_CHECKING

from sve_carddb.registry.inputs import Card

if TYPE_CHECKING:
    from sve_carddb.extract.official_jp import CardRecord

_LEGACY_TRAIT_PART = re.compile(r"ジオ・テオゴニア|[^・]+")


def legacy_projection(record: CardRecord) -> Card:
    """Apply the old Card input boundary, including its ignored extra fields."""
    card = Card.model_validate(asdict(record))
    # Confirmed identity receipts pin the old tokenizer, not the corrected face traits.
    for face, raw in zip(card.faces, record.faces, strict=True):
        face.traits = (
            [] if raw.trait_raw == "-" else _LEGACY_TRAIT_PART.findall(raw.trait_raw)
        )
    return card
