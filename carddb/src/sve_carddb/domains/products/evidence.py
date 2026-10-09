"""Resolve authored evidence through sealed archive closure, without live access."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.ingest.archive.frozen_sources import FrozenBatches

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.core.provenance import Source
    from sve_carddb.domains.products.models import Evidence


@dataclass(frozen=True)
class CheckedSource:
    source: Source


def resolve_evidence(
    references: tuple[Evidence, ...],
    stores: Mapping[str, Path],
    *,
    batches: FrozenBatches | None = None,
) -> Mapping[Evidence, CheckedSource]:
    """Verify each full batch before resolving its descriptor and first receipt."""
    batches = batches or FrozenBatches()
    result: dict[Evidence, CheckedSource] = {}
    for reference in references:
        if reference in result:
            continue
        key = reference.batch_id
        frozen = batches.configured(stores, key)
        if reference.source_version_id not in frozen.entries:
            raise ValueError(
                "Product evidence source version is absent from pinned batch"
            )
        source, _, _ = frozen.read(
            reference.source_version_id, parser_version="archive-closure-v1"
        )
        result[reference] = CheckedSource(source)
    return result
