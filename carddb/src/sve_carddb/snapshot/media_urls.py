"""Pure public image key and versioned display URL selection."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import array, integer, object_value

if TYPE_CHECKING:
    from sve_carddb.snapshot.reader import Row


MAX_SAFE = 9007199254740991
SIZE_KEYS = ("art_m", "art_s", "card_l", "card_m", "card_s")


def _uint(value: int, *, positive: bool) -> int:
    if type(value) is not int or not (1 if positive else 0) <= value <= MAX_SAFE:
        raise ValueError("Image URL integer outside safe domain")
    return value


def image_path(int_id: int, ordinal: int, size: str) -> str:
    """Use permanent identities, never a card number or face array position."""
    _uint(int_id, positive=True)
    _uint(ordinal, positive=False)
    if size not in SIZE_KEYS:
        raise ValueError("Unknown image size")
    suffix = "" if ordinal == 0 else "-f" + str(ordinal)
    return "images/" + size + "/" + str(int_id) + suffix + ".webp"


def image_url(int_id: int, ordinal: int, size: str, version: int) -> str:
    """The query version belongs to the card or art group, not the file key."""
    _uint(version, positive=True)
    return image_path(int_id, ordinal, size) + "?v=" + str(version)


def display_url(printing: Row, face: Row, media: Row, size: str) -> str | None:
    """Select the group's version only after validating the exact printing face."""
    if media["printing_id"] != printing["id"] or media["face_id"] != face["id"]:
        raise ValueError("Image URL media belongs to another printing face")
    if size not in SIZE_KEYS:
        raise ValueError("Unknown image size")
    if media["publication_state"] != "approved" or media["availability"] != "available":
        return None
    if size not in {object_value(v)["size_key"] for v in array(media["variants"])}:
        raise ValueError("Image URL size has no verified display variant")
    version = integer(
        media["art_version" if size.startswith("art_") else "card_version"]
    )
    return image_url(
        integer(printing["int_id"]), integer(face["ordinal"]), size, version
    )
