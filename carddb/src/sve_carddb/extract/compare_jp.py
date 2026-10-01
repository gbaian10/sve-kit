"""Read-only JP extractor comparison against legacy cards and registry evidence."""

import argparse
import hashlib
import json
import re
import sys
import tempfile
from collections import Counter, defaultdict
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import TYPE_CHECKING, cast
from urllib.parse import parse_qs, urlsplit

from pydantic import JsonValue, TypeAdapter

from sve_carddb.crawl import card_numbers
from sve_carddb.extract.jsonl import extract_cards
from sve_carddb.extract.official_jp import CardRecord
from sve_carddb.manifest import Manifest
from sve_carddb.registry.inputs import JSON_VALUE, Card, canonical, digest
from sve_carddb.registry.review import observation
from sve_carddb.registry.storage import load
from sve_carddb.source_archive import ArchiveReader

if TYPE_CHECKING:
    from collections.abc import Iterator

RECORD = TypeAdapter(CardRecord)
OBSERVATION_FIELDS = frozenset(
    {"region", "card_no", "recipe", "observation_hash", "rules_hash"}
)
APPROVED_NEW_FIELDS = frozenset(
    {"card.products", "card.related_cards", "card.faces[].trait_raw"}
)
# Each approved semantic change must name one card, field, and both value hashes.
APPROVED_RAW_FIELD_CHANGES: frozenset[tuple[str, str, str, str]] = frozenset()
_LEGACY_TRAIT_PART = re.compile(r"ジオ・テオゴニア|[^・]+")


@dataclass
class RawChanges:
    differences: list[dict[str, JsonValue]]
    added: list[str]
    removed: list[str]
    array_added: list[str]
    array_removed: list[str]


def legacy_projection(record: CardRecord) -> Card:
    """Apply the old Card input boundary, including its ignored extra fields."""
    card = Card.model_validate(asdict(record))
    # Confirmed identity receipts pin the old tokenizer, not the corrected face traits.
    for face, raw in zip(card.faces, record.faces, strict=True):
        face.traits = (
            [] if raw.trait_raw == "-" else _LEGACY_TRAIT_PART.findall(raw.trait_raw)
        )
    return card


def _read_legacy(
    path: Path,
) -> tuple[dict[str, Card], dict[str, dict[str, JsonValue]]]:
    cards: dict[str, Card] = {}
    raw: dict[str, dict[str, JsonValue]] = {}
    for line in path.read_bytes().splitlines():
        card = Card.model_validate_json(line)
        value = JSON_VALUE.validate_json(line)
        if not isinstance(value, dict):
            msg = f"legacy JP card is not an object: {card.number}"
            raise TypeError(msg)
        if card.number in cards:
            msg = f"duplicate legacy JP card: {card.number}"
            raise ValueError(msg)
        cards[card.number] = card
        raw[card.number] = value
    return cards, raw


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


def _raw_walk(
    before: JsonValue,
    after: JsonValue,
    path: str,
    changes: RawChanges,
) -> None:
    if isinstance(before, dict) and isinstance(after, dict):
        changes.added.extend(
            f"{path}.{key}" for key in sorted(after.keys() - before.keys())
        )
        changes.removed.extend(
            f"{path}.{key}" for key in sorted(before.keys() - after.keys())
        )
        for key in sorted(before.keys() & after.keys()):
            _raw_walk(before[key], after[key], f"{path}.{key}", changes)
    elif isinstance(before, list) and isinstance(after, list):
        for index in range(min(len(before), len(after))):
            _raw_walk(
                before[index],
                after[index],
                f"{path}[{index}]",
                changes,
            )
        changes.array_added.extend(
            f"{path}[{index}]" for index in range(len(before), len(after))
        )
        changes.array_removed.extend(
            f"{path}[{index}]" for index in range(len(after), len(before))
        )
    elif before != after or type(before) is not type(after):
        changes.differences.append(
            {
                "field": path,
                "old": _summary(before),
                "new": _summary(after),
                "transition": _transition(before, after),
            }
        )


def _transition(before: JsonValue, after: JsonValue) -> str:
    def state(value: JsonValue) -> str:
        if value is None:
            return "null"
        if value == "":  # ruff: ignore[compare-to-empty-string] -- distinguish empty text from other falsey JSON values
            return "empty"
        return "value"

    return f"{state(before)}->{state(after)}"


def _raw_comparison(before: dict[str, JsonValue], after: CardRecord) -> RawChanges:
    changes = RawChanges([], [], [], [], [])
    _raw_walk(before, asdict(after), "card", changes)
    return changes


def _approved_raw_change(number: str, change: dict[str, JsonValue]) -> bool:
    old = change["old"]
    new = change["new"]
    if not isinstance(old, dict) or not isinstance(new, dict):
        return False
    return (
        number,
        str(change["field"]),
        str(old["sha256"]),
        str(new["sha256"]),
    ) in APPROVED_RAW_FIELD_CHANGES


def _approved_new_field(path: str) -> bool:
    return re.sub(r"\[\d+\]", "[]", path) in APPROVED_NEW_FIELDS


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
    legacy_raw: dict[str, JsonValue] | None,
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
    raw = (
        _raw_comparison(legacy_raw, candidate)
        if legacy_raw is not None and candidate is not None
        else RawChanges([], [], [], [], [])
    )
    unexpected_raw = (
        any(not _approved_raw_change(number, diff) for diff in raw.differences)
        or any(not _approved_new_field(field) for field in raw.added)
        or bool(raw.removed or raw.array_added or raw.array_removed)
    )
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
        else "raw_field_difference"
        if unexpected_raw
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
        "raw_field_diffs": raw.differences,
        "added_fields": raw.added,
        "removed_fields": raw.removed,
        "array_added_items": raw.array_added,
        "array_removed_items": raw.array_removed,
        "envelope_diffs": envelope_diffs,
        "envelope_paths": [path for path, _ in envelopes],
    }


def _raw_field_summary(rows: list[dict[str, object]]) -> dict[str, dict[str, object]]:
    cards: dict[str, set[str]] = defaultdict(set)
    transitions: dict[str, Counter[str]] = defaultdict(Counter)
    for row in rows:
        number = str(row["card_no"])
        for change in cast("list[dict[str, JsonValue]]", row["raw_field_diffs"]):
            field = str(change["field"])
            cards[field].add(number)
            transitions[field][str(change["transition"])] += 1
    return {
        field: {
            "count": len(numbers),
            "card_numbers": sorted(numbers),
            "transitions": dict(sorted(transitions[field].items())),
        }
        for field, numbers in sorted(cards.items())
    }


def _field_presence_summary(
    rows: list[dict[str, object]], key: str
) -> dict[str, dict[str, object]]:
    cards: dict[str, set[str]] = defaultdict(set)
    for row in rows:
        for field in cast("list[str]", row[key]):
            cards[field].add(str(row["card_no"]))
    return {
        field: {"count": len(numbers), "card_numbers": sorted(numbers)}
        for field, numbers in sorted(cards.items())
    }


def _hint_summary(candidates: dict[str, CardRecord]) -> dict[str, object]:
    product_hrefs = sorted(
        (number, href)
        for number, card in candidates.items()
        for product in card.products
        for href in product.links
    )
    related_hrefs = sorted(
        (number, related.href)
        for number, card in candidates.items()
        for related in card.related_cards
    )
    self_links = sorted(
        {
            number
            for number, href in related_hrefs
            if number in parse_qs(urlsplit(href).query).get("cardno", [])
        }
    )
    related_paths = Counter(urlsplit(href).path for _, href in related_hrefs)
    non_card_links = [
        {"card_no": number, "href": href}
        for number, href in related_hrefs
        if urlsplit(href).path != "/cardlist/"
        or urlsplit(href).hostname not in {None, "shadowverse-evolve.com"}
        or not parse_qs(urlsplit(href).query).get("cardno")
    ]
    return {
        "product_cards": sum(bool(card.products) for card in candidates.values()),
        "related_cards": sum(bool(card.related_cards) for card in candidates.values()),
        "product_href_count": len(product_hrefs),
        "related_href_count": len(related_hrefs),
        "product_href_examples": [
            {"card_no": number, "href": href} for number, href in product_hrefs[:8]
        ],
        "related_href_examples": [
            {"card_no": number, "href": href} for number, href in related_hrefs[:8]
        ],
        "self_related_cards": self_links,
        "related_href_paths": dict(sorted(related_paths.items())),
        "non_card_related_hrefs": non_card_links,
    }


def compare(
    legacy_path: Path,
    candidate_path: Path,
    authored_root: Path,
    expected_numbers: set[str],
) -> dict[str, object]:
    """Compare every expected JP number and all authored JP observation envelopes."""
    old, old_raw = _read_legacy(legacy_path)
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
            old_raw.get(number),
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
        "raw_field_changes": _raw_field_summary(rows),
        "added_fields": _field_presence_summary(rows, "added_fields"),
        "removed_fields": _field_presence_summary(rows, "removed_fields"),
        "array_added_items": _field_presence_summary(rows, "array_added_items"),
        "array_removed_items": _field_presence_summary(rows, "array_removed_items"),
        "hints": _hint_summary(candidates),
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
