"""Read-only JP extractor comparison against legacy cards and registry evidence."""

import argparse
import hashlib
import json
import sys
import tempfile
from collections import Counter
from dataclasses import asdict
from pathlib import Path
from typing import TYPE_CHECKING, cast

from pydantic import JsonValue, TypeAdapter

from sve_carddb.crawl import card_numbers
from sve_carddb.extract.jsonl import extract_cards
from sve_carddb.extract.official_jp import CardRecord
from sve_carddb.manifest import Manifest
from sve_carddb.registry.inputs import Card, canonical, digest
from sve_carddb.registry.review import observation
from sve_carddb.registry.storage import load
from sve_carddb.source_archive import ArchiveReader

if TYPE_CHECKING:
    from collections.abc import Iterator

RECORD = TypeAdapter(CardRecord)
OBSERVATION_FIELDS = frozenset(
    {"region", "card_no", "recipe", "observation_hash", "rules_hash"}
)


def legacy_projection(record: CardRecord) -> Card:
    """Apply the old Card input boundary, including its ignored extra fields."""
    return Card.model_validate(asdict(record))


def _read_legacy(path: Path) -> dict[str, Card]:
    cards: dict[str, Card] = {}
    for line in path.read_bytes().splitlines():
        card = Card.model_validate_json(line)
        if card.number in cards:
            msg = f"duplicate legacy JP card: {card.number}"
            raise ValueError(msg)
        cards[card.number] = card
    return cards


def _read_candidates(path: Path) -> dict[str, CardRecord]:
    records: dict[str, CardRecord] = {}
    for line in path.read_bytes().splitlines():
        record = RECORD.validate_json(line)
        if record.number in records:
            msg = f"duplicate candidate JP card: {record.number}"
            raise ValueError(msg)
        records[record.number] = record
    return records


def _observations(
    value: JsonValue, path: str
) -> Iterator[tuple[str, str, dict[str, JsonValue]]]:
    if isinstance(value, dict):
        if value.get("region") == "jp" and (
            "recipe" in value or "observation_hash" in value
        ):
            number = value.get("card_no")
            if not isinstance(number, str):
                msg = f"JP observation at {path} has no card_no"
                raise ValueError(msg)
            yield number, path, value
        for key, child in value.items():
            yield from _observations(child, f"{path}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            yield from _observations(child, f"{path}[{index}]")


def _summary(value: JsonValue) -> dict[str, JsonValue]:
    encoded = canonical(value)
    return {
        "type": type(value).__name__,
        "bytes": len(encoded),
        "sha256": "sha256:" + hashlib.sha256(encoded).hexdigest(),
    }


def _diff(before: JsonValue, after: JsonValue, path: str) -> list[dict[str, JsonValue]]:
    if before == after and type(before) is type(after):
        return []
    if isinstance(before, dict) and isinstance(after, dict):
        result: list[dict[str, JsonValue]] = []
        for key in sorted(before.keys() | after.keys()):
            child_path = f"{path}.{key}"
            if key not in before or key not in after:
                result.append(
                    {
                        "field": child_path,
                        "old": _summary(before[key]) if key in before else None,
                        "new": _summary(after[key]) if key in after else None,
                    }
                )
            else:
                result.extend(_diff(before[key], after[key], child_path))
        return result
    if isinstance(before, list) and isinstance(after, list):
        result = []
        for index in range(max(len(before), len(after))):
            child_path = f"{path}[{index}]"
            if index >= len(before) or index >= len(after):
                result.append(
                    {
                        "field": child_path,
                        "old": _summary(before[index]) if index < len(before) else None,
                        "new": _summary(after[index]) if index < len(after) else None,
                    }
                )
            else:
                result.extend(_diff(before[index], after[index], child_path))
        return result
    return [{"field": path, "old": _summary(before), "new": _summary(after)}]


def _envelope_differences(
    envelopes: list[tuple[str, dict[str, JsonValue]]],
    old: dict[str, JsonValue] | None,
    new: dict[str, JsonValue] | None,
) -> list[dict[str, JsonValue]]:
    return [
        {
            "source": path,
            "projection": kind,
            "field": field,
            "envelope": envelope.get(field),
            "actual": actual.get(field),
        }
        for path, envelope in envelopes
        for kind, actual in (("legacy", old), ("candidate", new))
        if actual is not None
        for field in sorted(OBSERVATION_FIELDS)
        if envelope.get(field) != actual.get(field)
    ]


def _card_row(
    number: str,
    legacy: Card | None,
    candidate: CardRecord | None,
    envelopes: list[tuple[str, dict[str, JsonValue]]],
    expected: bool,
) -> dict[str, object]:
    projected = legacy_projection(candidate) if candidate is not None else None
    old_observation = observation(legacy, "jp") if legacy is not None else None
    new_observation = observation(projected, "jp") if projected is not None else None
    field_diffs = (
        _diff(legacy.model_dump(mode="json"), projected.model_dump(mode="json"), "card")
        if legacy is not None and projected is not None
        else []
    )
    envelope_diffs = _envelope_differences(envelopes, old_observation, new_observation)
    status = (
        "unexpected"
        if not expected
        else "missing_legacy"
        if legacy is None
        else "missing_candidate"
        if candidate is None
        else "missing_envelope"
        if not envelopes
        else "envelope_difference"
        if envelope_diffs
        else "projection_difference"
        if field_diffs
        else "exact"
    )
    return {
        "card_no": number,
        "status": status,
        "legacy": old_observation,
        "candidate": new_observation,
        "candidate_projection_sha256": (
            digest({"projection": "jp-extractor-v1", "record": asdict(candidate)})
            if candidate is not None
            else None
        ),
        "field_diffs": field_diffs,
        "envelope_diffs": envelope_diffs,
        "envelope_paths": [path for path, _ in envelopes],
    }


def compare(
    legacy_path: Path,
    candidate_path: Path,
    authored_root: Path,
    expected_numbers: set[str],
) -> dict[str, object]:
    """Compare every expected JP number and all authored JP observation envelopes."""
    old = _read_legacy(legacy_path)
    candidates = _read_candidates(candidate_path)
    _, entries = load(authored_root)
    envelopes: dict[str, list[tuple[str, dict[str, JsonValue]]]] = {}
    for entry in entries.values():
        for number, path, envelope in _observations(entry.data, entry.record_key):
            envelopes.setdefault(number, []).append((path, envelope))
    numbers = sorted(set(old) | set(candidates) | set(envelopes) | expected_numbers)
    rows = [
        _card_row(
            number,
            old.get(number),
            candidates.get(number),
            envelopes.get(number, []),
            number in expected_numbers,
        )
        for number in numbers
    ]
    counts = Counter(str(row["status"]) for row in rows)
    field_counts = Counter(
        str(field["field"])
        for row in rows
        for field in cast("list[dict[str, JsonValue]]", row["field_diffs"])
    )
    return {
        "complete": (
            bool(expected_numbers)
            and set(old) == expected_numbers
            and set(candidates) == expected_numbers
            and set(envelopes) == expected_numbers
            and counts == Counter({"exact": len(numbers)})
        ),
        "counts": dict(sorted(counts.items())),
        "denominators": {
            "legacy": len(old),
            "candidate": len(candidates),
            "envelope_cards": len(envelopes),
            "expected": len(expected_numbers),
            "union": len(numbers),
        },
        "coverage": {
            "missing_legacy": sorted(expected_numbers - old.keys()),
            "missing_candidate": sorted(expected_numbers - candidates.keys()),
            "missing_envelope": sorted(expected_numbers - envelopes.keys()),
            "unexpected_legacy": sorted(old.keys() - expected_numbers),
            "unexpected_candidate": sorted(candidates.keys() - expected_numbers),
            "unexpected_envelope": sorted(envelopes.keys() - expected_numbers),
        },
        "field_counts": dict(sorted(field_counts.items())),
        "cards": rows,
        "inputs": {
            "legacy_sha256": _file_hash(legacy_path),
            "candidate_sha256": _file_hash(candidate_path),
            "authored_index_sha256": _file_hash(authored_root / "ids" / "index.yaml")
            if (authored_root / "ids" / "index.yaml").exists()
            else None,
        },
    }


def _file_hash(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    """Verify one sealed batch and report without changing raw inputs or decisions."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--legacy", required=True, type=Path)
    parser.add_argument("--authored", required=True, type=Path)
    parser.add_argument("--store", required=True, type=Path)
    parser.add_argument("--store-id", required=True)
    parser.add_argument("--batch-id", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if args.output.resolve().is_relative_to(args.store.resolve()):
        msg = "comparison output must be outside the immutable archive"
        raise ValueError(msg)
    reader = ArchiveReader(args.store, args.store_id, args.batch_id)
    batch_dir = args.store / "batches" / args.batch_id.removeprefix("sha256:")
    with Manifest.open_snapshot(batch_dir / "manifest.sqlite") as snapshot:
        expected = set(card_numbers(snapshot))
        with tempfile.TemporaryDirectory() as work:
            candidate = Path(work) / "candidate.jsonl"
            extraction = extract_cards(snapshot, reader, candidate)
            report = compare(args.legacy, candidate, args.authored, expected)
    report["extraction"] = {
        "written": extraction.written,
        "missing": extraction.missing,
        "failed_numbers": sorted(extraction.failed),
    }
    report["complete"] = (
        report["complete"] is True and not extraction.missing and not extraction.failed
    )
    report["sealed_batch"] = {
        "batch_id": args.batch_id,
        "inventory_sha256": _file_hash(batch_dir / "inventory.json"),
    }
    args.output.write_text(
        json.dumps(report, ensure_ascii=False, sort_keys=True, indent=2) + "\n",
        encoding="utf-8",
    )
    sys.stdout.write(
        json.dumps({"complete": report["complete"], "counts": report["counts"]}) + "\n"
    )


if __name__ == "__main__":
    main()
