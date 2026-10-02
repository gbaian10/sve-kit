"""Stream explicit sealed announcement batches without latest cache or live state."""

from typing import TYPE_CHECKING

from sve_carddb.card_extras.errata_parser import PARSER, parse_notice
from sve_carddb.frozen_sources import FrozenSources

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

    from sve_carddb.card_extras.errata_parser import StagedNotice
    from sve_carddb.registry.records import Region


class FrozenErrataNotices:
    def __init__(
        self, root: Path, store_id: str, batch_id: str, *, region: Region
    ) -> None:
        if region not in {"jp", "en"}:
            raise ValueError("Errata archive requires an explicit JP or EN region")
        self.sources = FrozenSources(root, store_id, batch_id)
        self.region = region

    def notices(self) -> Iterator[StagedNotice]:
        """Retain every historical version and verify each source before parsing."""
        for entry in self.sources.inventory.entries:
            source, raw, descriptor = self.sources.read(
                entry.source_version_id, parser_version=PARSER
            )
            if (descriptor.provider, descriptor.kind) != (
                self.region,
                "errata",
            ):
                raise ValueError("Errata batch source identity mismatch")
            yield parse_notice(raw, source, region=self.region)
