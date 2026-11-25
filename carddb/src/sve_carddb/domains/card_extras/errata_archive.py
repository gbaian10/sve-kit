"""Stage one explicit announcement from a sealed archive for human review."""

import json
from typing import TYPE_CHECKING

from sve_carddb.domains.card_extras.errata_parser import PARSER, parse_notice
from sve_carddb.domains.card_extras.reviewed_errata import Evidence
from sve_carddb.ingest.archive.frozen_sources import FrozenSources

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.regions import Region


def stage(
    root: Path, store_id: str, batch_id: str, version: str, *, region: Region
) -> str:
    """Emit portable evidence and staging diagnostics without adopting any changes."""
    sources = FrozenSources(root, store_id, batch_id)
    source, raw, descriptor = sources.read(version, parser_version=PARSER)
    if (descriptor.provider, descriptor.kind) != (region, "errata"):
        raise ValueError("Errata batch source identity mismatch")
    notice = parse_notice(raw, source, region=region)
    pin = source.archive
    evidence = Evidence(
        id=source.id,
        url=source.url,
        sha256=source.sha256,
        fetched_at=source.fetched_at,
        etag=source.etag,
        last_modified=source.last_modified,
        parser_version=source.parser_version,
        batch_id=pin.batch_id,
        descriptor_sha256=pin.descriptor_sha256,
        first_receipt_id=pin.first_receipt_id,
    )
    return (
        json.dumps(
            {
                "source": evidence.model_dump(mode="json"),
                "region": notice.region,
                "announced_on": notice.heading_date,
                "date_raw": notice.heading_date_raw,
                "listed_card_numbers": notice.listed.numbers,
                "listed_card_name": notice.listed.name,
                "blocks": [
                    block.evidence.model_dump(mode="json") for block in notice.blocks
                ],
                "end_bases": [block.end_basis for block in notice.blocks],
                "outside_text": notice.outside_text,
                "issues": notice.issues,
            },
            ensure_ascii=False,
            indent=2,
        )
        + "\n"
    )
