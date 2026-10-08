"""Memory-only current catalog and exact vocabulary bindings."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.domains.catalog.models import Catalog
    from sve_carddb.domains.text_observations.vocabulary import Vocabulary


@dataclass(frozen=True)
class CatalogProjection:
    catalog: Catalog
    vocabulary: Vocabulary
