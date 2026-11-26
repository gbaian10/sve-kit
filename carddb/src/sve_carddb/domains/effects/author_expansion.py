"""Walk and validate materialized author trees."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import array, object_value
from sve_carddb.domains.effects.author_inputs import AuthorSchemas

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.domains.effects.author_validation import PathParts


def ability_nodes(tree: JsonValue) -> Iterator[tuple[PathParts, dict[str, JsonValue]]]:
    """Visit explicit card/face containers without guessing metadata key names."""
    cards = object_value(object_value(tree)["cards"])
    for key, card_value in cards.items():
        card = object_value(card_value)
        for index, ability in enumerate(array(card.get("abilities", []))):
            yield ("cards", key, "abilities", index), object_value(ability)
        for face_id, face_value in object_value(card.get("faces", {})).items():
            face = object_value(face_value)
            for index, ability in enumerate(array(face["abilities"])):
                yield (
                    ("cards", key, "faces", face_id, "abilities", index),
                    object_value(ability),
                )


class AuthorExpander:
    def __init__(self, schema_directory: Path) -> None:
        self.schemas = AuthorSchemas(schema_directory)
