"""Exact, single-decode card-route-v1 path segments."""

import re
import unicodedata
from urllib.parse import quote, unquote_to_bytes

RESERVED = frozenset({"unimplemented", "_provisional"})


def check_key(key: str) -> None:
    """Reject keys that cannot roundtrip as one UTF-8 path segment."""
    if not key or "/" in key or "\x00" in key:
        raise ValueError("Invalid card route key")
    key.encode("utf-8", errors="strict")


def encode_segment(key: str) -> str:
    """Encode exact bytes with ASCII unreserved characters and uppercase escapes."""
    check_key(key)
    return quote(key, safe="-._~", encoding="utf-8", errors="strict")


def decode_segment(segment: str) -> str:
    """Decode once, refusing malformed percent escapes and invalid UTF-8."""
    if re.search(r"%(?![0-9a-fA-F]{2})", segment):
        raise ValueError("Malformed card route escape")
    key = unquote_to_bytes(segment).decode("utf-8", errors="strict")
    check_key(key)
    return key


def folded_key(key: str) -> str:
    """Normalize only the secondary lookup index; exact keys remain untouched."""
    return unicodedata.normalize("NFKC", key).casefold()


def card_path(namespace: str, key: str) -> str:
    """Build a canonical printing entry without a slug or interface language."""
    if namespace == "official":
        if key in RESERVED:
            raise ValueError("Reserved official card route")
        return "/cards/" + encode_segment(key)
    if namespace != "provisional" or not re.fullmatch(r"[1-9][0-9]*", key):
        raise ValueError("Invalid provisional card route")
    return "/cards/_provisional/" + key
