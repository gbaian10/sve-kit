"""Read every current official card page in an explicitly pinned frozen batch."""

from typing import TYPE_CHECKING

from sve_carddb.domains import physical_sources
from sve_carddb.domains.products.official import PARSER, ProductPage
from sve_carddb.ingest.archive.frozen_sources import FrozenSources

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.regions import Region


class FrozenProducts:
    def __init__(
        self,
        root: Path,
        store_id: str,
        batch_id: str,
        *,
        sources: FrozenSources | None = None,
        region: Region,
    ) -> None:
        self.sources = sources or FrozenSources(root, store_id, batch_id)
        self.sources.require_scope(root, store_id, batch_id)
        self.region = region

    def pages(self) -> tuple[ProductPage, ...]:
        """Read current product projections from the shared verified physical pages."""
        pages: list[ProductPage] = []
        for entry in self.sources.inventory.current:
            _, descriptor = self.sources.metadata(
                entry.source_version_id, parser_version=PARSER
            )
            if (descriptor.provider, descriptor.kind, descriptor.url) != (
                self.region,
                "card",
                entry.url,
            ):
                raise ValueError("Official product batch source identity mismatch")
            page = physical_sources.page(
                self.sources,
                entry.source_version_id,
                region=self.region,
                parser_version=PARSER,
            )
            page.require_identity()
            pages.append(page.products.get())
        return tuple(pages)
