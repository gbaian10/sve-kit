"""Resolve authored evidence through sealed archive closure, without live access."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.frozen_sources import FrozenSources

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.build_inputs import Source
    from sve_carddb.products.models import Evidence


@dataclass(frozen=True)
class CheckedSource:
    source: Source


def resolve_evidence(
    references: tuple[Evidence, ...], stores: Mapping[str, Path]
) -> Mapping[Evidence, CheckedSource]:
    """Verify each full batch before resolving its descriptor and first receipt."""
    batches: dict[tuple[str, str], FrozenSources] = {}
    result: dict[Evidence, CheckedSource] = {}
    for reference in references:
        if reference in result:
            continue
        root = stores.get(reference.store_id)
        if root is None:
            raise ValueError(
                "Product evidence requires an explicitly configured archive store"
            )
        key = reference.store_id, reference.batch_id
        if key not in batches:
            batches[key] = FrozenSources(root, *key)
        if reference.source_version_id not in batches[key].entries:
            raise ValueError(
                "Product evidence source version is absent from pinned batch"
            )
        source, _, _ = batches[key].read(
            reference.source_version_id, parser_version="archive-closure-v1"
        )
        result[reference] = CheckedSource(source)
    return result
