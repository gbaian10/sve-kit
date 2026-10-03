"""Canonical URL form, used as the manifest primary key."""

import re
from urllib.parse import parse_qsl, quote, urlencode, urlsplit, urlunsplit

# RFC 3986 unreserved characters plus the sub-delims and separators that are safe in a path.
_PATH_SAFE = frozenset(
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~!$&'()*+,;=:@/"
)
_PERCENT_ESCAPE = re.compile(r"%[0-9A-Fa-f]{2}")
_DEFAULT_PORTS = {"http": 80, "https": 443}


def canonicalize(url: str) -> str:
    """Return the canonical form of an absolute http(s) URL.

    Lowercases scheme and host, drops default ports and fragments, sorts query
    parameters and percent-encodes non-ASCII characters and spaces. Existing
    percent-escapes are kept, so encoding twice gives the same result.
    """
    parts = urlsplit(url.strip())
    scheme = parts.scheme.lower()
    if scheme not in _DEFAULT_PORTS:
        msg = f"not an http(s) URL: {url!r}"
        raise ValueError(msg)
    host = (parts.hostname or "").lower()
    if not host:
        msg = f"URL has no host: {url!r}"
        raise ValueError(msg)
    netloc = (
        host if parts.port in {None, _DEFAULT_PORTS[scheme]} else f"{host}:{parts.port}"
    )
    path = _encode_path(parts.path or "/")
    query = urlencode(
        sorted(parse_qsl(parts.query, keep_blank_values=True)), quote_via=quote
    )
    return urlunsplit((scheme, netloc, path, query, ""))


def _encode_path(path: str) -> str:
    out: list[str] = []
    i = 0
    while i < len(path):
        escape = _PERCENT_ESCAPE.match(path, i)
        if escape:
            out.append(escape.group().upper())
            i = escape.end()
            continue
        char = path[i]
        out.append(char if char in _PATH_SAFE else quote(char, safe=""))
        i += 1
    return "".join(out)
