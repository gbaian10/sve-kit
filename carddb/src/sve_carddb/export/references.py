"""Public ID relationships shared by producers and read-only file closure checks."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import array, string

if TYPE_CHECKING:
    from collections.abc import Iterator

    from pydantic import JsonValue


REFERENCES = {
    "card_id": "card",
    "from_card_id": "card",
    "to_card_id": "card",
    "old_card_id": "card",
    "new_card_id": "card",
    "face_id": "face",
    "printing_id": "printing",
    "default_printing_id": "printing",
    "art_id": "art",
    "artist_id": "artist",
    "stamp_id": "stamp",
    "home_set_id": "product_family",
    "product_id": "product",
    "translation_id": "translation",
    "annotation_set_id": "annotation_set",
    "name_concept_id": "annotation_concept",
    "qa_id": "qa",
    "qa_version_id": "qa_version",
    "current_version_id": "qa_version",
    "cr_version_id": "cr_version",
    "cr_clause_id": "cr_clause",
    "revision_id": "face_revision",
    "replacement_revision_id": "ruling_revision",
    "rules_name_id": "rules_name",
    "profile_id": "rules_profile",
    "digital_card_id": "digital_card",
    "digital_art_id": "digital_art",
    "interaction_target_id": "digital_card",
    "voice_id": "voice",
    "keyword_id": "keyword",
}
ARRAY_REFERENCES = {
    "family_ids": "product_family",
    "cards": "card",
    "card_ids": "card",
    "debut_product_ids": "product",
    "active_scopes": "text_unit",
    "ruling_revision_ids": "ruling_revision",
    "complete_keyword_ids": "keyword",
    "partial_keyword_ids": "keyword",
    "undated_printing_ids": "printing",
}


def references(value: JsonValue) -> Iterator[tuple[str, str]]:
    """Walk every nested ID, including IDs held by wording and history records."""
    if isinstance(value, list):
        for item in value:
            yield from references(item)
    if isinstance(value, dict):
        for field, item in value.items():
            target = (
                "text_unit" if field.endswith("_unit_id") else REFERENCES.get(field)
            )
            if target is not None and item is not None:
                yield target, string(item)
            if field in ARRAY_REFERENCES:
                for identifier in array(item):
                    yield ARRAY_REFERENCES[field], string(identifier)
            yield from references(item)
