"""Read every current official card page in an explicitly pinned frozen batch."""

from typing import TYPE_CHECKING

from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.products.official import PARSER, ProductPage, parse_products

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.records import Region


class FrozenProducts:
    def __init__(
        self, root: Path, store_id: str, batch_id: str, *, region: Region
    ) -> None:
        self.sources = FrozenSources(root, store_id, batch_id)
        self.region = region

    def pages(self) -> tuple[ProductPage, ...]:
        """Read only batch current entries, rechecking raw and descriptor on access."""
        pages: list[ProductPage] = []
        for entry in self.sources.inventory.current:
            source, raw, descriptor = self.sources.read(
                entry.source_version_id, parser_version=PARSER
            )
            if (descriptor.provider, descriptor.kind, descriptor.url) != (
                self.region,
                "card",
                entry.url,
            ):
                raise ValueError("Official product batch source identity mismatch")
            pages.append(parse_products(raw, source, self.region))
        return tuple(pages)
