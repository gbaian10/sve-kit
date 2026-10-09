"""Independent expected Cards from the extractor's complete JSONL shape."""

from dataclasses import asdict
from typing import TYPE_CHECKING

from sve_carddb.domains.registry.inputs import Card

if TYPE_CHECKING:
    from sve_carddb.parse.pages import extract_en, extract_jp


def parsed_card(record: extract_jp.CardRecord | extract_en.CardRecord) -> Card:
    return Card.model_validate(asdict(record))
