"""Build permanent identities from explicitly reviewed grouping candidates."""

from collections import Counter, defaultdict
from dataclasses import dataclass, field
from typing import TYPE_CHECKING
from uuid import UUID, uuid5

from pydantic import JsonValue

from sve_carddb.registry.allocation import cursors, region_range
from sve_carddb.registry.evidence import require_evidence
from sve_carddb.registry.inputs import Card, digest, validate_mapping
from sve_carddb.registry.review import Inputs, observation, validation_cards
from sve_carddb.registry.storage import Entry

if TYPE_CHECKING:
    from sve_carddb.registry.review import Correction

NAMESPACE = UUID("e304714a-f18c-5fb6-a987-222988ffbb7a")


def permanent_id(kind: str, anchor: str) -> str:
    """Allocate opaque UUIDv5 IDs once; existing registries always take priority."""
    return kind + ":" + uuid5(NAMESPACE, kind + "\0" + anchor).hex


def string(data: dict[str, JsonValue], key: str) -> str:
    """Read typed registry fields without coercing malformed values."""
    value = data[key]
    if not isinstance(value, str) or not value:
        raise ValueError(f"Expected nonempty string: {key}")
    return value


def allocation_order(printing: Entry) -> tuple[str, ...]:
    """Stable first-batch order; card numbers compare as exact code-point strings."""
    data = printing.data
    return (
        string(data, "region"),
        printing.owner,
        string(data, "card_no"),
        string(data, "variant_key"),
        string(data, "id"),
    )


def owner(number: str) -> str:
    """Choose an immutable filing bucket, not a product inclusion assertion."""
    return number.split("-", maxsplit=1)[0]


@dataclass
class Builder:
    inputs: Inputs
    existing: dict[str, Entry]
    entries: dict[str, Entry] = field(default_factory=dict)
    parents: dict[tuple[str, str], str] = field(default_factory=dict)
    faces: dict[str, list[str]] = field(default_factory=dict)
    homes: dict[str, str] = field(default_factory=dict)

    def add(
        self, kind: str, identifier: str, home: str, data: dict[str, JsonValue]
    ) -> None:
        """Reject inconsistent repeated definitions before writing any files."""
        entry = Entry.model_validate(
            {
                "record_key": kind + ":" + identifier,
                "kind": kind,
                "owner": home,
                "data": data,
            }
        )
        if entry.record_key in self.entries and self.entries[entry.record_key] != entry:
            raise ValueError(f"Conflicting definition: {entry.record_key}")
        self.entries[entry.record_key] = entry

    def identities(self) -> None:
        """Apply reviewed structural groups, preserving every old parent and ID."""
        jp, en = validation_cards(self.inputs)
        validate_mapping(jp, en, self.inputs.mapping)
        groups: dict[str, list[tuple[str, Card]]] = defaultdict(list)
        jp_keys: dict[str, str] = {}
        for number, card in sorted(jp.items()):
            key = self.inputs.receipt.separate_groups.get(
                "jp:" + number, card.structure_hash("jp")
            )
            jp_keys[number] = "jp:" + key
            groups[jp_keys[number]].append(("jp", self.inputs.jp[number]))
        for number, card in sorted(en.items()):
            target = self.inputs.mapping.targets[number]
            key = (
                jp_keys[target]
                if target is not None
                else "en:"
                + self.inputs.receipt.separate_groups.get(
                    "en:" + number, card.structure_hash("en")
                )
            )
            groups[key].append(("en", self.inputs.en[number]))
        for group in groups.values():
            self._identity(
                sorted(group, key=lambda item: (item[0] != "jp", item[1].number))
            )

    def _identity(self, group: list[tuple[str, Card]]) -> None:
        previous = {
            string(entry.data, "card_id")
            for region, card in group
            if (
                entry := self.existing.get(
                    "printing:" + permanent_id("p", region + ":" + card.number)
                )
            )
            is not None
        }
        if len(previous) > 1:
            raise ValueError(
                "Grouping would merge permanent card IDs; identity_change required"
            )
        region, first = group[0]
        card_id = (
            next(iter(previous))
            if previous
            else permanent_id("c", region + ":" + first.number)
        )
        old = self.existing.get("card:" + card_id)
        home = old.owner if old else owner(first.number)
        face_ids = [
            permanent_id("f", card_id + ":" + str(index))
            for index in range(len(first.faces))
        ]
        self.faces[card_id], self.homes[card_id] = face_ids, home
        self.add(
            "card",
            card_id,
            home,
            {
                "id": card_id,
                "layout": "single" if len(face_ids) == 1 else "double_faced",
                "identity_state": "confirmed",
                "home_set_id": home,
            },
        )
        for index, face_id in enumerate(face_ids):
            self.add(
                "face",
                face_id,
                home,
                {
                    "id": face_id,
                    "card_id": card_id,
                    "ordinal": index,
                    "side": "front" if index == 0 else "back",
                },
            )
        for printing_region, card in group:
            self.parents[printing_region, card.number] = card_id
            self._printing(printing_region, card, card_id, face_ids)

    def _printing(
        self, region: str, card: Card, card_id: str, face_ids: list[str]
    ) -> None:
        if len(face_ids) != len(card.faces):
            raise ValueError(f"Face coverage mismatch: {region}:{card.number}")
        identifier = permanent_id("p", region + ":" + card.number)
        target = (
            self.inputs.mapping.targets.get(card.number) if region == "en" else None
        )
        data: dict[str, JsonValue] = {
            "id": identifier,
            "card_id": card_id,
            "region": region,
            "card_no": card.number,
            "variant_key": "standard",
            "home_set_id": owner(card.number),
            "source_face_map": [
                {"source_index": index, "face_id": face_id}
                for index, face_id in enumerate(face_ids)
            ],
            "observation": observation(card, region),
        }
        if region == "en":
            data["cross_region_review"] = {
                "checked": True,
                "target_jp_card_no": target,
                "target_observation": observation(self.inputs.jp[target], "jp")
                if target is not None
                else None,
            }
        self.add("printing", identifier, owner(card.number), data)

    def allocations(self) -> None:
        """Keep old int_ids; append new ones above each region's high-water mark."""
        old = {
            string(entry.data, "printing_id"): entry
            for entry in self.existing.values()
            if entry.kind == "card_int_id"
        }
        printings = sorted(
            (entry for entry in self.entries.values() if entry.kind == "printing"),
            key=allocation_order,
        )
        regions = {
            string(entry.data, "id"): string(entry.data, "region")
            for entry in printings
        }
        allocated: dict[str, list[int]] = defaultdict(list)
        for identifier, entry in old.items():
            value = entry.data["int_id"]
            if identifier not in regions or type(value) is not int:
                raise ValueError(f"Allocation without printing: {entry.record_key}")
            allocated[regions[identifier]].append(value)
        next_ids = cursors(allocated)
        pending = [entry for entry in printings if string(entry.data, "id") not in old]
        for region, count in Counter(
            string(entry.data, "region") for entry in pending
        ).items():
            if next_ids[region] + count - 1 > region_range(region).end:
                raise ValueError(f"int_id range exhausted for region {region}")
        for entry in old.values():
            self.entries[entry.record_key] = entry
        for printing in pending:
            identifier, region = (
                string(printing.data, "id"),
                string(printing.data, "region"),
            )
            self.add(
                "card_int_id",
                identifier,
                printing.owner,
                {
                    "int_id": next_ids[region],
                    "printing_id": identifier,
                    "allocated_at": self.inputs.receipt.reviewed_on,
                },
            )
            next_ids[region] += 1

    def curation(self) -> None:
        """Register confirmed absence, English art, reskins and source corrections."""
        only: dict[str, list[str]] = defaultdict(list)
        for number, target in self.inputs.mapping.targets.items():
            if target is None:
                only[self.parents["en", number]].append(number)
        for card_id, numbers in sorted(only.items()):
            observations: list[JsonValue] = [
                observation(self.inputs.en[number], "en") for number in sorted(numbers)
            ]
            if (
                old := self.existing.get("region_mapping_review:" + card_id)
            ) is not None:
                require_evidence(old.data["observations"], observations)
                self.entries[old.record_key] = old
                continue
            self.add(
                "region_mapping_review",
                card_id,
                self.homes[card_id],
                {
                    "card_id": card_id,
                    "target_region": "jp",
                    "state": "confirmed_none",
                    "as_of": self.inputs.receipt.reviewed_on,
                    "coverage_scope": "All Japanese official card extractions in the reviewed input batch",
                    "coverage_hash": self.inputs.receipt.input_hashes["jp"],
                    "observations": observations,
                },
            )
        self._arts()
        self._related()
        for correction in self.inputs.receipt.corrections:
            self._correction(correction)

    def _arts(self) -> None:
        groups = self.inputs.receipt.art_groups
        grouped = [number for group in groups for number in group]
        if (
            len(grouped) != len(set(grouped))
            or not set(grouped) <= self.inputs.mapping.original_art
            or any(not group for group in groups)
        ):
            raise ValueError("Art groups must be nonempty disjoint reviewed members")
        groups = [sorted(group) for group in groups] + [
            [number]
            for number in sorted(self.inputs.mapping.original_art - set(grouped))
        ]
        for group in groups:
            number = group[0]
            card_id = self.parents["en", number]
            if any(self.parents["en", member] != card_id for member in group):
                raise ValueError("Art group crosses card identities")
            printing_id = permanent_id("p", "en:" + number)
            for index, face_id in enumerate(self.faces[card_id]):
                art_id = permanent_id("a", printing_id + ":" + str(index))
                self.add(
                    "art",
                    art_id,
                    owner(number),
                    {
                        "id": art_id,
                        "card_id": card_id,
                        "face_id": face_id,
                        "classification": "unclassified",
                        "uses": [
                            {
                                "printing_id": permanent_id("p", "en:" + member),
                                "face_id": face_id,
                            }
                            for member in group
                        ],
                        "observation": observation(self.inputs.en[number], "en"),
                    },
                )

    def _related(self) -> None:
        targets: dict[str, str] = {}
        for number, jp_number in sorted(self.inputs.mapping.reskins.items()):
            source, target = self.parents["en", number], self.parents["jp", jp_number]
            if source == target or (source in targets and targets[source] != target):
                raise ValueError("Invalid or ambiguous reskin target")
            targets[source] = target
        for source, target in targets.items():
            evidence: list[JsonValue] = []
            for (region, number), parent in sorted(self.parents.items()):
                if parent in {source, target}:
                    card = (self.inputs.jp if region == "jp" else self.inputs.en)[
                        number
                    ]
                    evidence.append(
                        {
                            "role": "from" if parent == source else "to",
                            **observation(card, region),
                        }
                    )
            identifier = permanent_id("r", source + ":" + target)
            if (old := self.existing.get("card_related:" + identifier)) is not None:
                require_evidence(old.data["evidence"], evidence)
                self.entries[old.record_key] = old
                continue
            self.add(
                "card_related",
                identifier,
                self.homes[source],
                {
                    "id": identifier,
                    "from_card_id": source,
                    "to_card_id": target,
                    "relation": "same_rules_reskin",
                    "source_kind": "authored",
                    "target_printing_id": None,
                    "suggested_count": None,
                    "dsl_id": None,
                    "evidence": evidence,
                },
            )

    def _correction(self, correction: Correction) -> None:
        card = (self.inputs.jp if correction.region == "jp" else self.inputs.en)[
            correction.card_no
        ]
        printing_id = permanent_id("p", correction.region + ":" + correction.card_no)
        face_id = self.faces[self.parents[correction.region, correction.card_no]][
            correction.face_index
        ]
        identifier = permanent_id("x", printing_id + ":" + correction.field)
        self.add(
            "source_correction",
            identifier,
            owner(correction.card_no),
            {
                "id": identifier,
                "printing_id": printing_id,
                "face_id": face_id,
                "field": correction.field,
                "expected_raw_value": correction.expected_raw_value,
                "corrected_value": correction.corrected_value,
                "expected_source_hash": digest(card.model_dump(mode="json")),
                "source_hash_recipe": "registry-observation-v1",
                "reason": correction.reason,
                "state": correction.state,
                "reported_to_official": False,
                "reported_on": None,
                "report_url": None,
                "evidence": [
                    {
                        "kind": "card_image",
                        "sha256": correction.image_sha256,
                        "image_src": card.faces[correction.face_index].image,
                        "region": correction.region,
                        "locator": correction.locator,
                    }
                ],
            },
        )


def build(inputs: Inputs, existing: dict[str, Entry]) -> list[Entry]:
    """Construct the complete candidate registry without filesystem mutations."""
    builder = Builder(inputs, existing)
    builder.identities()
    builder.allocations()
    builder.curation()
    return sorted(builder.entries.values(), key=lambda entry: entry.record_key)
