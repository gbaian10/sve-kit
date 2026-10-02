"""Offline checkpoint for immutable JP pages and an explicitly selected legacy catalog."""

import argparse
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.snapshot.values import canonical, digest, object_value
from sve_carddb.source_archive import ArchiveError
from sve_carddb.template_sources.checkpoint import compare, read_legacy
from sve_carddb.template_sources.inventory import coverage, scan_batch
from sve_carddb.template_sources.output import write
from sve_carddb.template_sources.pins import recipes

if TYPE_CHECKING:
    from pydantic import JsonValue


def run(args: argparse.Namespace) -> dict[str, JsonValue]:
    """Generate source candidates first; consult the draft only for comparison."""
    sources = FrozenSources(args.store, args.store_id, args.batch_id)
    pins = recipes(args.repository, args.code_revision)
    legacy = read_legacy(args.legacy)
    if len(legacy) != args.expected_templates:
        raise ValueError(
            "Legacy template count differs from the explicit checkpoint expectation"
        )
    scan = scan_batch(sources, repository=args.repository, pins=pins)
    report: dict[str, JsonValue] = {
        "checkpoint_format": 1,
        "adoption_status": "candidate_only",
        "coverage_boundary": "sealed_current_jp_ability_fields",
        "store_id": args.store_id,
        "batch_id": args.batch_id,
        "code_revision": args.code_revision,
        "legacy_file_hash": digest(args.legacy.read_bytes()),
        "source_coverage": coverage(scan),
        **compare(scan, legacy),
    }
    report["complete"] = all(
        object_value(report[key])["complete"] is True
        for key in ("source_coverage", "fingerprints", "legacy_member_coverage")
    )
    write(args.output, scan, report, inputs=(args.store, args.repository, args.legacy))
    return report


def main() -> None:
    """No settings/default cache, live manifest or network is available to this tool."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--store", required=True, type=Path)
    parser.add_argument("--store-id", required=True)
    parser.add_argument("--batch-id", required=True)
    parser.add_argument("--repository", required=True, type=Path)
    parser.add_argument("--code-revision", required=True)
    parser.add_argument("--legacy", required=True, type=Path)
    parser.add_argument("--expected-templates", required=True, type=int)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        report = run(args)
    except ValueError, LookupError, OSError, TypeError, ArchiveError:
        # Underlying parser/validation errors can contain official source text.
        sys.stderr.write(
            "Template checkpoint failed; immutable inputs or recipe could not be verified\n"
        )
        raise SystemExit(2) from None
    sys.stdout.write(
        canonical(
            {
                key: object_value(report[key])["complete"]
                for key in ("fingerprints", "source_coverage", "legacy_member_coverage")
            }
        ).decode()
        + "\n"
    )
    raise SystemExit(0 if report["complete"] is True else 1)


if __name__ == "__main__":
    main()
