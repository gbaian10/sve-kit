"""Project pure source provenance into build database rows."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Iterable

    from sve_carddb.build import Database, Value
    from sve_carddb.core.provenance import Source


def source_values(source: Source) -> dict[str, Value]:
    """Project only version metadata; parser provenance belongs to each use."""
    return {
        "id": source.id,
        "kind": source.kind,
        "url": source.url,
        "raw_locator": source.raw_locator,
        "sha256": source.sha256,
        "fetched_at": source.fetched_at,
        "etag": source.etag,
        "last_modified": source.last_modified,
        "parser_version": None,
        "authored_path": None,
        "authored_revision": None,
    }


def raw_values(sources: Iterable[Source]) -> dict[str, dict[str, Value]]:
    """Validate repeated version metadata before any database writes."""
    result: dict[str, dict[str, Value]] = {}
    for source in sources:
        values = source_values(source)
        if source.id in result and result[source.id] != values:
            raise ValueError("Conflicting raw source metadata")
        result[source.id] = values
    return result


def insert_raw_sources(db: Database, sources: Iterable[Source]) -> None:
    """Reuse exact metadata only; never discard a conflicting version or its provenance."""
    required = raw_values(sources)
    for values in required.values():
        try:
            db.insert_exact("source_record", values, ("id",))
        except ValueError as error:
            if str(error) != "Conflicting catalog row":
                raise
            raise ValueError("Conflicting raw source metadata") from None
