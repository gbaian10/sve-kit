"""Resolve selected field translations without replacing the source language or owner."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.core.json import array, object_value, string
from sve_carddb.export.project.translations import source_unit

if TYPE_CHECKING:
    from sve_carddb.export.project.source import Record


@dataclass(frozen=True)
class DisplayText:
    text: str | None
    lang: str | None
    translation_id: str | None
    missing_translation: bool


def display_text(
    view: dict[str, list[Record]],
    owner: Record,
    field: str,
    lang: str,
    *,
    ordinal: int | None = None,
) -> DisplayText:
    """Missing translation keeps the exact original; unknown original stays unknown."""
    texts = {string(row["id"]): row for row in view["text_unit"]}
    translations = {string(row["id"]): row for row in view["translation"]}
    for raw in array(owner["translations"]):
        binding = object_value(raw)
        if (binding["field"], binding["ordinal"], binding["target_lang"]) == (
            field,
            ordinal,
            lang,
        ):
            identifier = string(binding["translation_id"])
            translation = translations[identifier]
            text = texts[string(translation["text_unit_id"])]
            return DisplayText(
                string(text["text"]), string(text["lang"]), identifier, False
            )
    original_id = source_unit(owner, field, ordinal)
    if original_id is None:
        return DisplayText(None, None, None, True)
    original = texts[string(original_id)]
    return DisplayText(
        string(original["text"]),
        string(original["lang"]),
        None,
        original["lang"] != lang,
    )
