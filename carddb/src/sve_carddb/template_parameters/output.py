"""Atomic, hash-only private candidate evidence; no authored payloads or official text."""

import shutil
import tempfile
from pathlib import Path
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameters.inventory import lineage
from sve_carddb.template_sources.output import write as write_sources

if TYPE_CHECKING:
    from collections.abc import Iterable

    from pydantic import JsonValue

    from sve_carddb.template_parameters.inventory import Candidates
    from sve_carddb.template_sources.inventory import Scan


def _lines(path: Path, rows: Iterable[JsonValue]) -> dict[str, JsonValue]:
    with path.open("wb") as stream:
        count = 0
        for row in rows:
            stream.write(canonical(row) + b"\n")
            count += 1
    return {"exact_hash": digest(path.read_bytes()), "rows": count}


def write(
    output: Path,
    candidates: Candidates,
    scan: Scan,
    report: dict[str, JsonValue],
    *,
    inputs: tuple[Path, ...],
) -> None:
    """Publish only a new directory after every candidate and source proof is written."""
    target = output.resolve()
    if (
        any(p.is_symlink() for p in (output, *output.parents))
        or output.exists()
        or any(target.is_relative_to(p.resolve()) for p in inputs)
    ):
        raise ValueError(
            "Parameter output must be new and outside all immutable inputs"
        )
    output.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=".template-parameters-", dir=output.parent))
    try:
        write_sources(staging / "sources", scan, {}, inputs=inputs)
        files: dict[str, JsonValue] = {
            "candidates.jsonl": _lines(
                staging / "candidates.jsonl",
                (
                    item.model_dump(mode="json")
                    for item in sorted(candidates.entries, key=lambda c: c.inventory_id)
                ),
            ),
            "legacy-lineage.jsonl": _lines(
                staging / "legacy-lineage.jsonl", lineage(candidates)
            ),
            "term-mentions.jsonl": _lines(
                staging / "term-mentions.jsonl", candidates.mentions
            ),
            "field-spans.jsonl": _lines(
                staging / "field-spans.jsonl", candidates.field_proofs
            ),
            "rule-candidates.jsonl": _lines(
                staging / "rule-candidates.jsonl", candidates.rule_matches
            ),
        }
        report["files"] = files
        (staging / "report.json").write_bytes(canonical(report) + b"\n")
        staging.rename(output)
    finally:
        if staging.exists():
            shutil.rmtree(staging)
