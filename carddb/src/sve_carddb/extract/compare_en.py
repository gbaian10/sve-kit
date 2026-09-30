"""Measure English registry observations against one sealed raw batch."""

import argparse
import hashlib
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import TYPE_CHECKING, cast

from pydantic import JsonValue
from pydantic import ValidationError as ModelValidationError

from sve_carddb.fetch.validate import ValidationError, decode_html
from sve_carddb.fetch.writer import LocalState
from sve_carddb.html import (
    MissingElementError,
    attribute,
    parse,
    require_one,
    select_all,
    select_one,
)
from sve_carddb.registry.inputs import Card, canonical, digest, read_cards
from sve_carddb.registry.records import PrintingData
from sve_carddb.registry.review import observation
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.source_archive import ArchiveReader
from sve_carddb.sources import official_en as en
from sve_carddb.sources import official_jp as jp
from sve_carddb.sources.official_jp import MIN_PAGE_BYTES

if TYPE_CHECKING:
    from selectolax.lexbor import LexborNode

    from sve_carddb.extract.jsonl import RawReader

_STAT_HEADINGS = {
    "status-Item-Cost": "cost",
    "status-Item-Power": "power",
    "status-Item-Hp": "hp",
}
_WHITESPACE = re.compile(r"[ \t\r\f\v]+")
_INDEX = re.compile(r"\[\d+\]")


def parse_card(body: bytes, number: str) -> Card:
    """Build a candidate using only the EN page's typed registry fields."""
    en.parse_card(body, expected_number=number)
    detail = require_one(
        parse(decode_html(body, min_bytes=MIN_PAGE_BYTES)), ".cardlist-Detail"
    )
    faces: list[dict[str, object]] = []
    for inner in select_all(detail, ".cardlist-Detail_Box_Inner"):
        info: dict[str, str] = {}
        for row in select_all(inner, ".info dl"):
            key = require_one(row, "dt").text(strip=True)
            if not key or key in info:
                msg = "missing or duplicate EN info key"
                raise ValidationError(msg)
            info[key] = _render(require_one(row, "dd"))
        stats: dict[str, str] = {}
        for item in select_all(inner, ".status-Item"):
            classes = (attribute(item, "class") or "").split()
            stat_key = next(
                (value for label, value in _STAT_HEADINGS.items() if label in classes),
                None,
            )
            if stat_key is None:
                continue
            heading = require_one(item, ".heading").text(strip=True)
            value = item.text(strip=True).removeprefix(heading).strip()
            if not value or stat_key in stats:
                msg = "missing or duplicate EN stat"
                raise ValidationError(msg)
            stats[stat_key] = value
        if set(stats) != {"cost", "power", "hp"}:
            msg = "EN card has incomplete stats"
            raise ValidationError(msg)
        image = attribute(require_one(inner, ".img img"), "src")
        if not image:
            msg = "EN card image has no src"
            raise ValidationError(msg)
        text = select_one(inner, ".detail")
        speech = select_one(inner, ".speech")
        faces.append(
            {
                "name": require_one(inner, ".ttl").text(strip=True),
                "info": info,
                "stats": stats,
                "text": _render(text) if text is not None else None,
                "speech": _render(speech) if speech is not None else None,
                "image": image,
            }
        )
    if not faces:
        msg = "EN card has no faces"
        raise ValidationError(msg)
    return Card.model_validate({"number": number, "faces": faces})


def _render(node: LexborNode) -> str:
    parts: list[str] = []
    _walk(node, parts)
    return "\n".join(
        _WHITESPACE.sub(" ", line).strip() for line in "".join(parts).split("\n")
    ).strip()


def _walk(node: LexborNode, parts: list[str]) -> None:
    for child in node.iter(include_text=True):
        if child.tag == "-text":
            parts.append((child.text_content or "").replace("\n", " "))
        elif child.tag == "br":
            parts.append("\n")
        elif child.tag == "img":
            parts.append("{" + (attribute(child, "alt") or "?") + "}")
        else:
            if (
                child.tag in {"p", "div"}
                and parts
                and not "".join(parts).endswith("\n")
            ):
                parts.append("\n")
            _walk(child, parts)


def _summary(value: JsonValue) -> dict[str, JsonValue]:
    return {
        "type": type(value).__name__,
        "bytes": len(canonical(value)),
        "sha256": digest(value),
    }


def _diff(before: JsonValue, after: JsonValue, path: str) -> list[dict[str, JsonValue]]:
    if before == after and type(before) is type(after):
        return []
    if isinstance(before, dict) and isinstance(after, dict):
        changes: list[dict[str, JsonValue]] = []
        for key in sorted(before.keys() | after.keys()):
            child = f"{path}.{key}"
            if key not in before or key not in after:
                changes.append(
                    {
                        "field": child,
                        "old": _summary(before[key]) if key in before else None,
                        "new": _summary(after[key]) if key in after else None,
                    }
                )
            else:
                changes.extend(_diff(before[key], after[key], child))
        return changes
    if isinstance(before, list) and isinstance(after, list):
        changes = []
        for index in range(max(len(before), len(after))):
            child = f"{path}[{index}]"
            if index >= len(before) or index >= len(after):
                changes.append(
                    {
                        "field": child,
                        "old": _summary(before[index]) if index < len(before) else None,
                        "new": _summary(after[index]) if index < len(after) else None,
                    }
                )
            else:
                changes.extend(_diff(before[index], after[index], child))
        return changes
    return [{"field": path, "old": _summary(before), "new": _summary(after)}]


def _length_distribution(items: list[dict[str, JsonValue]]) -> dict[str, object]:
    """Count canonical byte deltas; positive means the candidate is longer."""
    deltas: Counter[int] = Counter()
    unpaired = 0
    for item in items:
        old, new = item["old"], item["new"]
        if not isinstance(old, dict) or not isinstance(new, dict):
            unpaired += 1
            continue
        old_bytes, new_bytes = old.get("bytes"), new.get("bytes")
        if not isinstance(old_bytes, int) or not isinstance(new_bytes, int):
            unpaired += 1
            continue
        deltas[new_bytes - old_bytes] += 1
    return {
        "candidate_shorter": sum(count for delta, count in deltas.items() if delta < 0),
        "equal_length": deltas[0],
        "candidate_longer": sum(count for delta, count in deltas.items() if delta > 0),
        "unpaired": unpaired,
        "common_deltas": [
            {"candidate_minus_legacy_bytes": delta, "count": count}
            for delta, count in sorted(
                deltas.items(), key=lambda pair: (-pair[1], pair[0])
            )[:8]
        ],
    }


def measure(
    expected: list[tuple[str, str, str, str, str | None]],
    legacy: dict[str, Card],
    reader: RawReader,
) -> dict[str, object]:
    """Classify each registered EN printing exactly once, with input evidence."""
    rows: list[dict[str, object]] = []
    groups: dict[str, list[dict[str, JsonValue]]] = defaultdict(list)
    for (
        number,
        printing_id,
        registry_hash,
        registry_rules_hash,
        decision_id,
    ) in expected:
        old = legacy.get(number)
        old_observation = observation(old, "en") if old is not None else None
        row: dict[str, object] = {
            "card_no": number,
            "printing_id": printing_id,
            "decision_id": decision_id,
            "registry_observation_hash": registry_hash,
            "registry_rules_hash": registry_rules_hash,
            "legacy_observation_hash": old_observation["observation_hash"]
            if old_observation
            else None,
            "legacy_rules_hash": old_observation["rules_hash"]
            if old_observation
            else None,
            "candidate_observation_hash": None,
            "candidate_rules_hash": None,
            "field_diffs": [],
        }
        if old_observation is None or (
            old_observation["observation_hash"],
            old_observation["rules_hash"],
        ) != (registry_hash, registry_rules_hash):
            row["status"] = "no_corresponding_input"
            row["input_reason"] = (
                "missing_number" if old is None else "registry_hash_mismatch"
            )
        elif reader.local_state(en.card_url(number)) is not LocalState.TRUSTED:
            row["status"] = "missing_raw"
        else:
            body = reader.read(en.card_url(number))
            try:
                candidate = parse_card(body, number)
            except (
                ValidationError,
                MissingElementError,
                ModelValidationError,
                ValueError,
                TypeError,
            ) as exc:
                row["status"] = "parse_failed"
                row["error_type"] = type(exc).__name__
            else:
                actual = observation(candidate, "en")
                row["candidate_observation_hash"] = actual["observation_hash"]
                row["candidate_rules_hash"] = actual["rules_hash"]
                row["status"] = (
                    "exact"
                    if (actual["observation_hash"], actual["rules_hash"])
                    == (registry_hash, registry_rules_hash)
                    else "mismatch"
                )
                if row["status"] == "mismatch":
                    assert old is not None
                    diffs = _diff(
                        old.model_dump(mode="json"),
                        candidate.model_dump(mode="json"),
                        "card",
                    )
                    row["field_diffs"] = diffs
                    for diff in diffs:
                        group = _INDEX.sub("[]", str(diff["field"]))
                        groups[group].append({"card_no": number, **diff})
        rows.append(row)
    counts = Counter(str(row["status"]) for row in rows)
    comparable = counts["exact"] + counts["mismatch"]
    affected = defaultdict(set)
    for row in rows:
        if row["status"] != "exact" and row["decision_id"] is not None:
            affected[str(row["decision_id"])].add(str(row["card_no"]))
    return {
        "denominator": len(expected),
        "counts": {
            status: counts[status]
            for status in (
                "exact",
                "mismatch",
                "missing_raw",
                "parse_failed",
                "no_corresponding_input",
            )
        },
        "rates": {
            "all_registered": counts["exact"] / len(expected) if expected else None,
            "comparable": counts["exact"] / comparable if comparable else None,
            "comparable_denominator": comparable,
        },
        "field_groups": {
            field: {
                "difference_count": len(items),
                "card_count": len({str(item["card_no"]) for item in items}),
                "face_indexes": sorted(
                    {
                        int(match.group(1))
                        for item in items
                        if (match := re.search(r"faces\[(\d+)\]", str(item["field"])))
                        is not None
                    }
                ),
                "card_numbers": sorted({str(item["card_no"]) for item in items}),
                "length_delta_bytes": _length_distribution(items),
                "samples": items[:8],
            }
            for field, items in sorted(groups.items())
        },
        "affected_decisions": {
            key: {"card_count": len(numbers), "card_numbers": sorted(numbers)}
            for key, numbers in sorted(affected.items())
        },
        "cards": rows,
    }


def _hash(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    """Measure fixed EN observations without opening latest or live sources."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--legacy", required=True, type=Path)
    parser.add_argument("--authored", required=True, type=Path)
    parser.add_argument("--store", required=True, type=Path)
    parser.add_argument("--store-id", required=True)
    parser.add_argument("--batch-id", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if not args.output.resolve().is_relative_to(Path("/tmp")):  # ruff: ignore[hardcoded-temp-file] -- this one-off report must never enter source roots
        msg = "report must be written under /tmp"
        raise ValueError(msg)
    snapshot = load_registry(args.authored)
    expected = sorted(
        (
            data.card_no,
            data.id,
            data.observation.observation_hash,
            data.observation.rules_hash,
            record.decision_id,
        )
        for record in snapshot.records.values()
        if isinstance((data := record.data), PrintingData) and data.region == "en"
    )
    legacy = read_cards(args.legacy)
    reader = ArchiveReader(args.store, args.store_id, args.batch_id)
    report = measure(expected, legacy, reader)
    batch_dir = args.store / "batches" / args.batch_id.removeprefix("sha256:")
    source_files = [
        Path(__file__),
        Path(en.parse_card.__code__.co_filename),
        Path(jp.parse_card.__code__.co_filename),
        Path(parse.__code__.co_filename),
        Path(decode_html.__code__.co_filename),
        Path(canonical.__code__.co_filename),
        Path(observation.__code__.co_filename),
        Path(ArchiveReader.__init__.__code__.co_filename),
    ]
    parser_hashes = {path.name: _hash(path) for path in source_files}
    report["inputs"] = {
        "legacy_sha256": _hash(args.legacy),
        "archive_inventory_sha256": _hash(batch_dir / "inventory.json"),
        "archive_manifest_sha256": _hash(batch_dir / "manifest.sqlite"),
        "authored_index_sha256": _hash(args.authored / "ids" / "index.yaml"),
        "parser_file_hashes": parser_hashes,
        "parser_version_hash": digest(cast("JsonValue", parser_hashes)),
        "store_id": args.store_id,
        "batch_id": args.batch_id,
    }
    args.output.write_text(
        json.dumps(report, ensure_ascii=False, sort_keys=True, indent=2) + "\n",
        encoding="utf-8",
    )
    sys.stdout.write(
        json.dumps(
            {
                "denominator": report["denominator"],
                "counts": report["counts"],
                "rates": report["rates"],
            }
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
