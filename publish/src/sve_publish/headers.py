"""R2 transport metadata for validated public export members."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.export.read_api import Member

JSON_HEADERS = {
    "content-type": "application/json",
    "cache-control": "public,max-age=31536000,immutable",
}
IMAGE_HEADERS = {
    "content-type": "image/webp",
    "cache-control": "public,max-age=86400,must-revalidate",
}
INDEX_HEADERS = {"content-type": "application/json", "cache-control": "no-store"}


def member_headers(member: Member) -> dict[str, str]:
    """Derive transfer encoding from the verified sibling, never from local metadata."""
    return (
        JSON_HEADERS
        if member.encoding is None
        else JSON_HEADERS | {"content-encoding": member.encoding}
    )
