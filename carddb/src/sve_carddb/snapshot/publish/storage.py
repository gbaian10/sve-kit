"""Injected conditional object boundary; no credentials or network implementation."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Protocol

if TYPE_CHECKING:
    from contextlib import AbstractContextManager


class PublishError(ValueError):
    """Redacted publication failure safe for operator logs."""


@dataclass(frozen=True, repr=False)
class Stored:
    """An ETag is an opaque precondition, never a content digest."""

    raw: bytes
    etag: str
    headers: dict[str, str]


class ObjectStore(Protocol):
    """All publishers and collectors must share the same exclusive writer lease.

    Conditional PUT/DELETE still reject non-cooperating object writes. A real
    adapter must supply an equivalent cross-process lease, not a process mutex.
    No live adapter or live CLI is enabled by this package.
    """

    def exclusive(self) -> AbstractContextManager[None]:
        """Acquire the deployment-wide publisher/GC lease."""
        ...

    def get(self, key: str) -> Stored | None:
        """Return identity response bytes and opaque ETag, or missing."""
        ...

    def put(
        self, key: str, raw: bytes, headers: dict[str, str], *, expected: str | None
    ) -> bool:
        """Create only if expected is None, otherwise If-Match; false is 412."""
        ...

    def delete(self, key: str, *, expected: str) -> bool:
        """Delete only the object matching the supplied opaque ETag."""
        ...

    def keys(self, prefix: str) -> tuple[str, ...]:
        """List only under the exact explicitly permitted prefix."""
        ...


class Freshness(Protocol):
    """Ordinary CDN GET of the complete HTTPS URL, preserving its version query.

    Return decoded identity bytes or None for missing/opaque/redirect responses;
    adapters must not bypass caches. Purge is a separate operator permission.
    """

    def get(self, url: str) -> bytes | None:
        """Fetch the full query-bearing URL without bypassing any cache."""
        ...
