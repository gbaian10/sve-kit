"""Referential and allocation invariants independent of the generator."""

import re
from collections import defaultdict
from typing import TYPE_CHECKING

from sve_carddb.registry.allocation import (
    REGION_RANGES,
    cursors,
    region_allocations,
    region_range,
)
from sve_carddb.registry.build import string
from sve_carddb.registry.evidence import require_evidence

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.registry.storage import Entry


def validate(entries: list[Entry]) -> None:
    """Reject duplicate identities, dangling faces, incomplete mappings and IDs."""
    validate_structure(entries)
    validate_collections(entries)


def validate_structure(entries: list[Entry], *, historical_art: bool = False) -> None:
    """Check stable identities before review/relation closure is renewed by replay."""
    records = {entry.record_key: entry for entry in entries}
    if len(records) != len(entries):
        raise ValueError("Duplicate record keys")
    kinds: dict[str, dict[str, Entry]] = defaultdict(dict)
    for entry in entries:
        _fields(entry)
        if entry.kind in {
            "card",
            "face",
            "printing",
            "art",
            "card_related",
            "source_correction",
        }:
            identifier = string(entry.data, "id")
            if any(identifier in group for group in kinds.values()):
                raise ValueError(f"Duplicate permanent ID: {identifier}")
            kinds[entry.kind][identifier] = entry
    faces = kinds["face"]
    for entry in faces.values():
        if string(entry.data, "card_id") not in kinds["card"]:
            raise ValueError("Orphan face")
    card_faces: dict[str, dict[str, Entry]] = defaultdict(dict)
    for identifier, face in faces.items():
        card_faces[string(face.data, "card_id")][identifier] = face
    printings = kinds["printing"]
    seen: set[tuple[str, str, str]] = set()
    regions: dict[str, set[str]] = defaultdict(set)
    for entry in printings.values():
        _printing(entry, kinds, seen, card_faces)
        regions[string(entry.data, "card_id")].add(string(entry.data, "region"))
    _cross_region(kinds["printing"])
    _cards(kinds, card_faces)
    _allocations(entries, printings)
    for art in kinds["art"].values():
        _art(art, kinds, historical=historical_art)


def validate_collections(entries: list[Entry]) -> None:
    """Require complete review/relation sets after every atomic transition."""
    kinds: dict[str, dict[str, Entry]] = defaultdict(dict)
    regions: dict[str, set[str]] = defaultdict(set)
    for entry in entries:
        if "id" in entry.data:
            kinds[entry.kind][string(entry.data, "id")] = entry
        if entry.kind == "printing":
            regions[string(entry.data, "card_id")].add(string(entry.data, "region"))
    _reviews(entries, kinds["printing"], regions)
    _relations(kinds)


def _reviews(
    entries: list[Entry], printings: dict[str, Entry], regions: dict[str, set[str]]
) -> None:
    reviews = {
        string(entry.data, "card_id")
        for entry in entries
        if entry.kind == "region_mapping_review"
    }
    if reviews != {card_id for card_id, values in regions.items() if values == {"en"}}:
        raise ValueError("English-only identity review coverage mismatch")
    for entry in entries:
        if entry.kind == "region_mapping_review":
            if (
                entry.data["state"] != "confirmed_none"
                or entry.data["target_region"] != "jp"
            ):
                raise ValueError("Invalid English-only review")
            require_evidence(
                entry.data["observations"],
                [
                    printing.data["observation"]
                    for printing in printings.values()
                    if printing.data["card_id"] == entry.data["card_id"]
                ],
            )


def _relations(kinds: dict[str, dict[str, Entry]]) -> None:
    printings, faces = kinds["printing"], kinds["face"]
    targets: dict[str, str] = {}
    for related in kinds["card_related"].values():
        source, target = (
            string(related.data, "from_card_id"),
            string(related.data, "to_card_id"),
        )
        if (
            source == target
            or source not in kinds["card"]
            or target not in kinds["card"]
            or source in targets
        ):
            raise ValueError("Invalid or repeated reskin relation")
        if targets.get(target) == source:
            raise ValueError("Reverse reskin relationship")
        targets[source] = target
        evidence: list[JsonValue] = []
        for printing in printings.values():
            parent = string(printing.data, "card_id")
            if parent not in {source, target}:
                continue
            observed = printing.data["observation"]
            if not isinstance(observed, dict):
                raise TypeError("Invalid printing observation")
            evidence.append({"role": "from" if parent == source else "to", **observed})
        require_evidence(related.data["evidence"], evidence)
    for correction in kinds["source_correction"].values():
        printing = printings[string(correction.data, "printing_id")]
        face = faces[string(correction.data, "face_id")]
        if printing.data["card_id"] != face.data["card_id"]:
            raise ValueError("Correction face does not belong to its printing")


def _printing(
    entry: Entry,
    kinds: dict[str, dict[str, Entry]],
    seen: set[tuple[str, str, str]],
    card_faces: dict[str, dict[str, Entry]],
) -> None:
    data = entry.data
    key = string(data, "region"), string(data, "card_no"), string(data, "variant_key")
    if key in seen or key[0] not in REGION_RANGES:
        raise ValueError("Duplicate or invalid printing")
    seen.add(key)
    card_id = string(data, "card_id")
    if card_id not in kinds["card"]:
        raise ValueError("Orphan printing")
    expected = set(card_faces[card_id])
    maps = data["source_face_map"]
    if not isinstance(maps, list):
        raise TypeError("Invalid source face map")
    actual: set[str] = set()
    for index, mapping in enumerate(maps):
        if not isinstance(mapping, dict) or mapping["source_index"] != index:
            raise ValueError("Invalid source face order")
        actual.add(string(mapping, "face_id"))
    if actual != expected or len(maps) != len(expected):
        raise ValueError("Incomplete or duplicate face mapping")


def _cards(
    kinds: dict[str, dict[str, Entry]], card_faces: dict[str, dict[str, Entry]]
) -> None:
    for identifier, card in kinds["card"].items():
        faces = [entry.data for entry in card_faces[identifier].values()]
        expected = 1 if card.data["layout"] == "single" else 2
        if len(faces) != expected or {
            face["ordinal"] for face in faces if type(face["ordinal"]) is int
        } != set(range(expected)):
            raise ValueError("Card face cardinality mismatch")
        sides = {string(face, "side") for face in faces}
        if sides != ({"front"} if expected == 1 else {"front", "back"}):
            raise ValueError("Card face side mismatch")


def _allocations(entries: list[Entry], printings: dict[str, Entry]) -> None:
    integers: set[int] = set()
    allocated: set[str] = set()
    for entry in entries:
        if entry.kind != "card_int_id":
            continue
        value = entry.data["int_id"]
        printing = string(entry.data, "printing_id")
        if printing not in printings:
            raise ValueError("Printing allocation coverage mismatch")
        bounds = region_range(string(printings[printing].data, "region"))
        if (
            type(value) is not int
            or value not in bounds
            or value in integers
            or printing in allocated
        ):
            raise ValueError("Invalid, out-of-range or reused int_id allocation")
        integers.add(value)
        allocated.add(printing)
    if allocated != set(printings):
        raise ValueError("Printing allocation coverage mismatch")


def check_cursors(next_int_id: dict[str, int], entries: list[Entry]) -> None:
    """Each index cursor must equal its region's highest int_id + 1 (or the start)."""
    if next_int_id != cursors(region_allocations(entries)):
        raise ValueError("Allocation high-water mark mismatch")


def _art(
    entry: Entry, kinds: dict[str, dict[str, Entry]], *, historical: bool = False
) -> None:
    face = kinds["face"][string(entry.data, "face_id")]
    if face.data["card_id"] != entry.data["card_id"]:
        raise ValueError("Art face ownership mismatch")
    uses: JsonValue = entry.data["uses"]
    if not isinstance(uses, list) or (not uses and not historical):
        raise ValueError("Art must have reviewed printing uses")
    for use in uses:
        if not isinstance(use, dict):
            raise TypeError("Invalid art use")
        printing = kinds["printing"][string(use, "printing_id")]
        if (
            printing.data["region"] != "en"
            or printing.data["card_id"] != entry.data["card_id"]
            or use["face_id"] != entry.data["face_id"]
        ):
            raise ValueError("English original art attached to the wrong printing")


FIELDS = {
    "card": "id layout identity_state home_set_id",
    "face": "id card_id ordinal side",
    "printing": "id card_id region card_no variant_key home_set_id source_face_map observation",
    "card_int_id": "int_id printing_id allocated_at",
    "region_mapping_review": "card_id target_region state as_of coverage_scope coverage_hash observations",
    "art": "id card_id face_id classification uses observation",
    "card_related": "id from_card_id to_card_id relation source_kind target_printing_id suggested_count dsl_id evidence",
    "source_correction": "id printing_id face_id field expected_raw_value corrected_value expected_source_hash source_hash_recipe reason state reported_to_official reported_on report_url evidence",
}


def _fields(entry: Entry) -> None:
    expected = set(FIELDS[entry.kind].split())
    if entry.kind == "printing" and entry.data.get("region") == "en":
        expected.add("cross_region_review")
    if set(entry.data) != expected:
        raise ValueError(f"Invalid data fields: {entry.record_key}")
    key = (
        "printing_id"
        if entry.kind == "card_int_id"
        else "card_id"
        if entry.kind == "region_mapping_review"
        else "id"
    )
    identifier = string(entry.data, key)
    if (
        entry.record_key != entry.kind + ":" + identifier
        or re.fullmatch(r"[cfparx]:[0-9a-f]{32}", identifier) is None
    ):
        raise ValueError(f"Invalid record identity: {entry.record_key}")


def _cross_region(printings: dict[str, Entry]) -> None:
    jp = {
        string(entry.data, "card_no"): entry.data["card_id"]
        for entry in printings.values()
        if entry.data["region"] == "jp"
    }
    for entry in printings.values():
        if entry.data["region"] != "en":
            continue
        review = entry.data["cross_region_review"]
        if not isinstance(review, dict) or review.get("checked") is not True:
            raise ValueError("English printing lacks explicit checked review")
        target = review.get("target_jp_card_no")
        if target is not None and (
            not isinstance(target, str) or jp.get(target) != entry.data["card_id"]
        ):
            raise ValueError("English review target disagrees with printing parent")
