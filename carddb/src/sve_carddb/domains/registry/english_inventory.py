"""Offline EN candidate inventory; report conclusions never allocate or adopt IDs."""

import argparse
import sys
from collections import Counter
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Literal
from urllib.parse import parse_qs, urlsplit

from pydantic import Field, JsonValue

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import Hash, RecordData, Text, UInt
from sve_carddb.domains.registry.inputs import read_cards
from sve_carddb.domains.registry.projection import en_card, jp_card
from sve_carddb.domains.registry.records import MappingReviewData, PrintingData
from sve_carddb.domains.registry.review import observation
from sve_carddb.domains.registry.snapshot import RegistrySnapshot, load_registry
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.parse.pages import extract_en, extract_jp, official_en, official_jp

if TYPE_CHECKING:
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.registry.inputs import Card

PARSER = "english-identity-inventory-v1"
type Classification = Literal["has_jp", "confirmed_no_jp", "unresolved"]


class Conclusion(RecordData):
    """Private report input, deliberately separate from permanent registry adoption."""

    en_card_no: Text
    source_index: UInt
    en_observation_hash: Hash
    jp_coverage_hash: Hash
    classification: Classification
    reason: Text
    compared_fields: tuple[Text, ...] = Field(min_length=1)
    jp_card_no: Text | None = None
    jp_source_index: UInt | None = None
    jp_observation_hash: Hash | None = None


@dataclass(frozen=True)
class Batch:
    region: Region
    pin: dict[str, JsonValue]
    cards: dict[str, Card]
    sources: dict[str, dict[str, JsonValue]]
    failures: tuple[dict[str, JsonValue], ...] = ()

    @property
    def coverage_hash(self) -> str:
        """Pin the parsed complete input, without claiming it proves absence."""
        return digest(
            canonical(
                [
                    observation(card, self.region)
                    for _, card in sorted(self.cards.items())
                ]
            )
        )


@dataclass(frozen=True)
class JPBaseline:
    exact_sha256: str
    cards: dict[str, Card]


def read_jp_baseline(path: Path) -> JPBaseline:
    """Read the exact historic JSONL boundary, without treating it as current raw."""
    return JPBaseline(digest(path.read_bytes()), read_cards(path))


def _coverage(jp: Batch, reviewed: JPBaseline | None) -> dict[str, JsonValue]:
    if reviewed is None:
        return {"available": False, "matches_current": False}
    historic = {
        number: observation(card, "jp") for number, card in reviewed.cards.items()
    }
    current = {number: observation(card, "jp") for number, card in jp.cards.items()}
    changed = sorted(
        number
        for number in set(historic) | set(current)
        if historic.get(number) != current.get(number)
    )
    return {
        "available": True,
        "exact_sha256": reviewed.exact_sha256,
        "matches_current": not changed
        and not jp.failures
        and len(jp.cards) == jp.pin["expected_sources"],
        "changed_card_numbers": list[JsonValue](changed),
    }


def scan(sources: FrozenSources, region: Region) -> Batch:
    """Replay every current source in an explicit regional card batch, read-only."""
    if [(scope.provider, scope.kind) for scope in sources.inventory.scope] != [
        (region, "card")
    ]:
        raise ValueError("Inventory requires an exclusively regional card batch")
    cards: dict[str, Card] = {}
    proofs: dict[str, dict[str, JsonValue]] = {}
    failures: list[dict[str, JsonValue]] = []
    seen: set[str] = set()
    for current in sources.inventory.current:
        source, raw, descriptor = sources.read(
            current.source_version_id, parser_version=PARSER
        )
        if (descriptor.provider, descriptor.kind, descriptor.url, source.kind) != (
            region,
            "card",
            current.url,
            "official_page",
        ):
            raise ValueError("Inventory source identity mismatch")
        numbers = parse_qs(urlsplit(source.url).query).get("cardno", [])
        if len(numbers) != 1:
            raise ValueError("Inventory card URL lacks an exact number")
        number = numbers[0]
        if number in seen:
            raise ValueError("Duplicate inventory card number")
        seen.add(number)
        if source.url != (official_jp if region == "jp" else official_en).card_url(
            number
        ):
            raise ValueError("Inventory source URL mismatch")
        try:
            if region == "jp":
                jp_record = extract_jp.extract_card(raw, number=number)
                card = jp_card(jp_record)
                release_date, products = jp_record.release_date, jp_record.products
            else:
                en_record = extract_en.extract_card(raw, number=number)
                card = en_card(en_record)
                release_date, products = en_record.release_date, en_record.products
        except ValueError, LookupError, UnicodeError:
            failures.append(
                {
                    "card_no": number,
                    "source_version_id": source.id,
                    "reason": "source_projection_failed",
                }
            )
            continue
        cards[number] = card
        proofs[number] = {
            "batch_id": sources.batch_id,
            "source_version_id": source.id,
            "raw_sha256": source.sha256,
            "url": source.url,
            "fetched_at": source.fetched_at,
            "observation": observation(card, region),
            "release_date": release_date,
            "products_hash": digest(
                canonical([asdict(product) for product in products])
            ),
        }
    return Batch(
        region,
        {
            "store_id": sources.store_id,
            "batch_id": sources.batch_id,
            "created_at": sources.inventory.created_at,
            "parser": PARSER,
            "expected_sources": len(sources.inventory.current),
            "source_versions": [
                item.source_version_id for item in sources.inventory.current
            ],
            "history_gaps": len(sources.inventory.history_gaps),
        },
        cards,
        proofs,
        tuple(failures),
    )


def _fields(card: Card, index: int) -> dict[str, JsonValue]:
    face = card.faces[index]
    return {
        "locator": f"/faces/{index}",
        "image_url": face.image,
        "field_hashes": {
            key: digest(canonical(value))
            for key, value in face.model_dump(mode="json").items()
        },
    }


def _reason(conclusion: Conclusion, card: Card, jp: Batch) -> str | None:
    if (
        conclusion.en_observation_hash != observation(card, "en")["observation_hash"]
        or conclusion.jp_coverage_hash != jp.coverage_hash
    ):
        return "conclusion_input_changed"
    if conclusion.classification == "has_jp":
        target = jp.cards.get(conclusion.jp_card_no or "")
        index = conclusion.jp_source_index
        if (
            target is None
            or index is None
            or index >= len(target.faces)
            or conclusion.jp_observation_hash
            != observation(target, "jp")["observation_hash"]
        ):
            return "conclusion_jp_target_unavailable_or_changed"
    elif conclusion.classification == "confirmed_no_jp":
        if jp.failures or len(jp.cards) != jp.pin["expected_sources"]:
            return "jp_coverage_incomplete"
        if any(
            value is not None
            for value in (
                conclusion.jp_card_no,
                conclusion.jp_source_index,
                conclusion.jp_observation_hash,
            )
        ):
            return "absence_conclusion_has_target"
    return None


def _row(
    number: str,
    index: int,
    printing: PrintingData | None,
    jp: Batch,
    en: Batch,
    conclusion: Conclusion | None,
) -> dict[str, JsonValue]:
    card = en.cards.get(number)
    row: dict[str, JsonValue] = {
        "en_card_no": number,
        "source_index": index,
        "printing_id": printing.id if printing else None,
        "card_id": printing.card_id if printing else None,
        "face_id": next(
            (
                face.face_id
                for face in printing.source_face_map
                if face.source_index == index
            ),
            None,
        )
        if printing
        else None,
        "classification": "unresolved",
        "reason": "no_manual_identity_conclusion",
        "permanent_identity_change": False,
    }
    if card is None or index >= len(card.faces):
        row["reason"] = "en_source_or_face_unavailable"
        return row
    row["source"] = en.sources[number]
    row["face"] = _fields(card, index)
    if conclusion is None:
        return row
    reason = _reason(conclusion, card, jp)
    if reason is not None:
        row["reason"] = reason
        return row
    row.update(
        classification=conclusion.classification,
        reason=conclusion.reason,
        compared_fields=list(conclusion.compared_fields),
    )
    if conclusion.classification == "has_jp":
        assert conclusion.jp_card_no is not None
        assert conclusion.jp_source_index is not None
        row["jp_target"] = {
            "card_no": conclusion.jp_card_no,
            "source_index": conclusion.jp_source_index,
            "source": jp.sources[conclusion.jp_card_no],
            "face": _fields(
                jp.cards[conclusion.jp_card_no], conclusion.jp_source_index
            ),
        }
        row["permanent_identity_change"] = True
    return row


def _history(
    row: dict[str, JsonValue],
    review: MappingReviewData | None,
) -> None:
    if review:
        row["historical_review"] = review.model_dump(mode="json")
        if row["reason"] == "no_manual_identity_conclusion":
            row["reason"] = "historical_absence_requires_current_conclusion"
    if row["classification"] == "confirmed_no_jp":
        row["permanent_identity_change"] = review is None or row["face_id"] is None


def inventory(
    registry: RegistrySnapshot,
    jp: Batch,
    en: Batch,
    conclusions: tuple[Conclusion, ...] = (),
    jp_baseline: JPBaseline | None = None,
) -> dict[str, JsonValue]:
    """Classify every EN card/face lacking an explicit JP face in the registry.

    Historical confirmed-none records are context, never fresh translation eligibility.
    Same-name cards, suffixes, reskins and wording similarity do not establish identity.
    """
    if (jp.region, en.region) != ("jp", "en"):
        raise ValueError("Inventory requires JP and EN inputs in that order")
    printings = [
        record.data
        for record in registry.records.values()
        if isinstance(record.data, PrintingData)
    ]
    english = {item.card_no: item for item in printings if item.region == "en"}
    jp_faces = {
        face.face_id
        for item in printings
        if item.region == "jp"
        for face in item.source_face_map
    }
    reviews = {
        item.card_id: item
        for record in registry.records.values()
        if isinstance(item := record.data, MappingReviewData)
        and item.target_region == "jp"
    }
    coverage = _coverage(jp, jp_baseline)
    decisions = {(item.en_card_no, item.source_index): item for item in conclusions}
    if len(decisions) != len(conclusions):
        raise ValueError("Duplicate manual identity conclusion")
    rows: list[dict[str, JsonValue]] = []
    keys: set[tuple[str, int]] = set()
    for number in sorted(set(en.cards) | set(english)):
        printing = english.get(number)
        mapped = (
            {face.source_index: face.face_id for face in printing.source_face_map}
            if printing
            else {}
        )
        card = en.cards.get(number)
        indexes = set(range(len(card.faces))) if card else set()
        indexes.update(mapped)
        for index in sorted(indexes):
            if mapped.get(index) in jp_faces:
                continue
            keys.add((number, index))
            row = _row(number, index, printing, jp, en, decisions.get((number, index)))
            review = reviews.get(printing.card_id) if printing else None
            _history(row, review)
            rows.append(row)
    rows.extend(
        {
            "en_card_no": failure["card_no"],
            "source_index": None,
            "classification": "unresolved",
            "reason": "unparsed_en_face_inventory",
            "source": failure,
            "permanent_identity_change": False,
        }
        for failure in en.failures
        if failure["card_no"] not in english and failure["card_no"] not in en.cards
    )
    if set(decisions) - keys:
        raise ValueError("Manual conclusion is outside the candidate inventory")
    return {
        "format": 1,
        "purpose": "identity_inventory_only",
        "inputs": {
            "jp": {
                **jp.pin,
                "coverage_hash": jp.coverage_hash,
                "sources": list(jp.sources.values()),
            },
            "en": {
                **en.pin,
                "coverage_hash": en.coverage_hash,
                "sources": list(en.sources.values()),
            },
            "historical_jp_coverage": coverage,
            "registry_index_hash": digest(registry.files.index_content),
            "registry_shards": [
                {"path": shard.path, "exact_sha256": digest(shard.exact_content)}
                for shard in registry.files.shards
            ],
            "conclusions_hash": digest(
                canonical([item.model_dump(mode="json") for item in conclusions])
            ),
        },
        "counts": {
            status: sum(row["classification"] == status for row in rows)
            for status in ("has_jp", "confirmed_no_jp", "unresolved")
        },
        "unresolved_reasons": dict(
            Counter(
                str(row["reason"])
                for row in rows
                if row["classification"] == "unresolved"
            )
        ),
        "failures": [*jp.failures, *en.failures],
        "candidates": list[JsonValue](rows),
    }


def main() -> None:
    """Write a private reproducible report, without touching source or authored inputs."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--store-id", required=True)
    parser.add_argument("--jp-batch", required=True)
    parser.add_argument("--en-batch", required=True)
    parser.add_argument("--authored", type=Path, required=True)
    parser.add_argument("--conclusions", type=Path)
    parser.add_argument("--jp-baseline", type=Path)
    parser.add_argument("--program-revision", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    for supplied in (args.jp_baseline, args.conclusions):
        if supplied is not None and args.output.resolve() == supplied.resolve():
            raise ValueError("Report output must not overwrite review inputs")
    for protected in (args.archive, args.authored):
        if args.output.resolve().is_relative_to(protected.resolve()):
            raise ValueError("Report output must be outside source and authored inputs")
    conclusions = (
        tuple(
            Conclusion.model_validate_json(line)
            for line in args.conclusions.read_bytes().splitlines()
            if line
        )
        if args.conclusions
        else ()
    )
    jp = scan(FrozenSources(args.archive, args.store_id, args.jp_batch), "jp")
    en = scan(FrozenSources(args.archive, args.store_id, args.en_batch), "en")
    report = inventory(
        load_registry(args.authored),
        jp,
        en,
        conclusions,
        read_jp_baseline(args.jp_baseline) if args.jp_baseline else None,
    )
    report["program_revision"] = args.program_revision
    report["program_sha256"] = digest(Path(__file__).read_bytes())
    report["dependencies"] = {
        "carddb/uv.lock": digest((Path(__file__).parents[4] / "uv.lock").read_bytes())
    }
    args.output.write_bytes(canonical(report) + b"\n")
    sys.stdout.write(canonical(report["counts"]).decode() + "\n")


if __name__ == "__main__":
    main()
