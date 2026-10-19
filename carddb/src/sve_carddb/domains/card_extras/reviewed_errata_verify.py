"""Explicit admission-time checks against immutable archived announcement bytes."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import digest
from sve_carddb.domains.card_extras.reviewed_errata import Block, admit
from sve_carddb.ingest.archive.frozen_sources import FrozenSources

if TYPE_CHECKING:
    from pathlib import Path


def verify_ranges(raw: bytes, blocks: tuple[Block, ...]) -> int:
    """Reject invalid UTF-8 byte coordinates without confusing them with character offsets."""
    for block in blocks:
        for span in (block.raw_before, block.raw_after):
            if not 0 <= span.start < span.end <= len(raw):
                raise ValueError("Errata evidence range is out of bounds")
            if digest(raw[span.start : span.end]) != span.sha256:
                raise ValueError("Errata evidence range hash differs")
    return 2 * len(blocks)


def verify(root: Path, archive: Path, store_id: str) -> dict[str, int]:
    """Read pinned frozen batches only; never write an archive or use latest cache."""
    notices = admit(root)
    batches: dict[str, FrozenSources] = {}
    ranges = 0
    for notice in notices:
        pin = notice.source
        if pin.batch_id not in batches:
            batches[pin.batch_id] = FrozenSources(archive, store_id, pin.batch_id)
        source, raw, _ = batches[pin.batch_id].read(
            pin.id, parser_version=pin.parser_version
        )
        if source != pin.source(store_id):
            raise ValueError("Errata frozen source evidence differs")
        ranges += verify_ranges(raw, notice.blocks)
    return {"notices": len(notices), "ranges": ranges}
