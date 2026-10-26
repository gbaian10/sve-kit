"""Offline entry point for identity registry generation."""

import argparse
import fcntl
import json
import sys
from collections import Counter
from datetime import date
from pathlib import Path
from typing import TYPE_CHECKING

from sve_carddb.domains.registry.build import build
from sve_carddb.domains.registry.corrections import project_corrections
from sve_carddb.domains.registry.review import read_inputs
from sve_carddb.domains.registry.storage import load, plan_files, write_files
from sve_carddb.domains.registry.transitions.renewal import load_renewed_registry
from sve_carddb.domains.registry.transitions.writer import Recipe
from sve_carddb.domains.registry.validate import check_cursors, validate

if TYPE_CHECKING:
    from sve_carddb.domains.registry.snapshot import RegistrySnapshot
    from sve_carddb.domains.registry.storage import Entry, Index


def _registry(
    root: Path, inputs: Path | None
) -> tuple[Index, dict[str, Entry], RegistrySnapshot | None]:
    if inputs is None:
        index, existing = load(root)
        return index, existing, None
    recipe = Recipe.model_validate_json(inputs.read_bytes())
    if recipe.repo.resolve() != root.parent.resolve():
        raise ValueError("Renewal recipe must name this authored repository")
    reviewed = load_renewed_registry(root, recipe.evidence())
    return (
        reviewed.files.index(),
        {
            entry.record_key: entry
            for shard in reviewed.files.shards
            for entry in shard.envelope().records
        },
        reviewed,
    )


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
    parser.add_argument(
        "--renewal-inputs",
        type=Path,
        help="Verify existing review/art transitions with an explicit renewal recipe before appending",
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
        index, existing, reviewed = _registry(root, args.renewal_inputs)
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
            reviewed=reviewed,
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
