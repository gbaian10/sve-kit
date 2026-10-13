"""Synthetic public media rows independent of the text build database."""

from pydantic import JsonValue


def media_rows() -> dict[str, list[dict[str, JsonValue]]]:
    return {
        "image_asset": [
            {
                "id": "image",
                "source_src_raw": "../source.png",
                "source_url": "https://example.invalid/source.png",
                "width": 459,
                "height": 641,
                "format": "png",
            }
        ],
        "printing_image": [
            {
                "printing_id": "printing",
                "face_id": "face",
                "image_id": "image",
                "publication_state": "pending",
                "availability": "unfetched",
                "card_version": None,
                "art_version": None,
                "variants": [],
            }
        ],
        "image_variant": [],
    }
