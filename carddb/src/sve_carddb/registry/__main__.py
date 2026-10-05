"""Offline entry point for identity registry generation."""

import argparse
import fcntl
import json
import sys
from collections import Counter
from datetime import date
from pathlib import Path

from sve_carddb.registry.build import build
from sve_carddb.registry.corrections import project_corrections
from sve_carddb.registry.review import read_inputs
from sve_carddb.registry.storage import load, plan_files, write_files
from sve_carddb.registry.validate import check_cursors, validate


def main() -> None:
    """Validate all input and planned output before installing any shards."""
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "jp",
        "en",
        "candidates",
        "confirmations",
        "original-art",
        "decisions",
        "images",
        "authored",
    ):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument(
        "--as-of",
        type=date.fromisoformat,
        required=True,
        help="Date of the JP input batch, recorded on new confirmed-absence reviews",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Require an already complete, unchanged registry",
    )
    parser.add_argument(
        "--corrections-output",
        type=Path,
        help="Write field-local build projections and correction markers",
    )
    args = parser.parse_args()
    root: Path = args.authored
    cache = root.parent / "carddb" / ".cache"
    cache.mkdir(parents=True, exist_ok=True)
    with (cache / "identity-registry.lock").open("w", encoding="utf-8") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        paths = {
            "jp": args.jp,
            "en": args.en,
            "candidates": args.candidates,
            "confirmations": args.confirmations,
            "original_art": args.original_art,
        }
        inputs = read_inputs(paths, args.decisions, args.images, as_of=args.as_of)
        index, existing = load(root)
        if existing:
            validate(list(existing.values()))
        check_cursors(index.next_int_id, list(existing.values()))
        entries = build(inputs, existing)
        validate(entries)
        corrections = project_corrections(inputs, entries)
        if any(row["status"] == "conflict" for row in corrections):
            raise ValueError("Source correction conflict; requires re-review")
        files = plan_files(
            root,
            entries,
            loaded=(index, existing),
        )
        if args.check and files:
            raise ValueError("Registry is incomplete; generation would append files")
        if not args.check:
            write_files(files)
        if args.corrections_output is not None:
            payload = (
                json.dumps(corrections, ensure_ascii=False, sort_keys=True, indent=2)
                + "\n"
            )
            if args.check:
                if args.corrections_output.read_text(encoding="utf-8") != payload:
                    raise ValueError("Correction projection differs from current build")
            else:
                args.corrections_output.parent.mkdir(parents=True, exist_ok=True)
                args.corrections_output.write_text(payload, encoding="utf-8")
        sys.stdout.write(
            json.dumps(
                {
                    "records": dict(
                        sorted(Counter(entry.kind for entry in entries).items())
                    ),
                    "written_files": len(files),
                    "correction_applications": dict(
                        sorted(
                            Counter(str(row["status"]) for row in corrections).items()
                        )
                    ),
                },
                sort_keys=True,
            )
            + "\n"
        )


if __name__ == "__main__":
    main()
