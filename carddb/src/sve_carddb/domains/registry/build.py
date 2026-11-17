"""Build permanent identities from explicitly reviewed grouping candidates."""

from collections import Counter, defaultdict
from dataclasses import dataclass, field
from typing import TYPE_CHECKING
from uuid import UUID, uuid5

from pydantic import JsonValue

from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.registry.allocation import cursors, region_range
from sve_carddb.domains.registry.evidence import require_evidence
from sve_carddb.domains.registry.inputs import Card, validate_mapping
from sve_carddb.domains.registry.review import Inputs, observation, validation_cards
from sve_carddb.domains.registry.storage import Entry

if TYPE_CHECKING:
    from collections.abc import Iterable

    from sve_carddb.domains.registry.review import Correction

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


class _Joins:
    """Disjoint sets of grouping keys."""

    def __init__(self, keys: Iterable[str]) -> None:
        self.root = {key: key for key in keys}

    def find(self, key: str) -> str:
        while self.root[key] != key:
            self.root[key] = self.root[self.root[key]]
            key = self.root[key]
        return key

    def union(self, left: str, right: str) -> None:
        self.root[self.find(left)] = self.find(right)


@dataclass
class Builder:
    inputs: Inputs
    existing: dict[str, Entry]
    entries: dict[str, Entry] = field(default_factory=dict)
    parents: dict[tuple[str, str], str] = field(default_factory=dict)
    faces: dict[str, list[str]] = field(default_factory=dict)
    homes: dict[str, str] = field(default_factory=dict)
    # Existing card ID folded into a reviewed same-card group -> the kept card ID.
    merged: dict[str, str] = field(default_factory=dict)

    def add(
        self, kind: str, identifier: str, home: str, data: dict[str, JsonValue]
    ) -> None:
        """Reject inconsistent repeated definitions before writing any files."""
        entry = Entry.model_validate(
            {
                "kind": kind,
                "owner": home,
                "data": data,
            }
        )
        if entry.record_key != kind + ":" + identifier:
            raise ValueError("Registry identifier disagrees with data")
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
            key = self.inputs.decisions.separate_groups.get(
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
                + self.inputs.decisions.separate_groups.get(
                    "en:" + number, card.structure_hash("en")
                )
            )
            groups[key].append(("en", self.inputs.en[number]))
        for group in self._joined(groups):
            self._identity(
                sorted(group, key=lambda item: (item[0] != "jp", item[1].number))
            )

    def _existing_card(self, region: str, number: str) -> str | None:
        entry = self.existing.get(
            "printing:" + permanent_id("p", region + ":" + number)
        )
        return None if entry is None else string(entry.data, "card_id")

    def _joined(
        self, groups: dict[str, list[tuple[str, Card]]]
    ) -> list[list[tuple[str, Card]]]:
        """Join structure groups that already share a card or a reviewed same-card group.

        A later reprint whose fingerprint matches a folded printing thus lands on the
        kept card without a new decision.
        """
        joins = _Joins(groups)
        by_card: dict[str, str] = {}
        by_number: dict[str, str] = {}
        for key, members in groups.items():
            for region, card in members:
                by_number[region + ":" + card.number] = key
                if (existing := self._existing_card(region, card.number)) is not None:
                    joins.union(key, by_card.setdefault(existing, key))
        self._check_groups()
        for keep, numbers in self.inputs.decisions.same_cards.items():
            if keep not in by_card or any(
                number not in by_number for number in numbers
            ):
                raise ValueError(f"Same-card group has unknown members: {keep}")
            for number in numbers:
                joins.union(by_number[number], by_card[keep])
        joined: dict[str, list[tuple[str, Card]]] = defaultdict(list)
        for key, members in groups.items():
            joined[joins.find(key)].extend(members)
        return list(joined.values())

    def folded_faces(self) -> dict[str, str]:
        """Each folded card's face -> the kept card's face with the same ordinal."""
        result: dict[str, str] = {}
        for entry in self.existing.values():
            if entry.kind == "face" and (
                keep := self.merged.get(string(entry.data, "card_id"))
            ):
                ordinal = entry.data["ordinal"]
                if not isinstance(ordinal, int) or ordinal >= len(self.faces[keep]):
                    raise ValueError("Folded card has a face the kept card lacks")
                result[string(entry.data, "id")] = self.faces[keep][ordinal]
        return result

    def _check_groups(self) -> None:
        """Groups stay disjoint, so a mistake is reported as such, not as a missing group."""
        groups = self.inputs.decisions.same_cards
        members = [number for numbers in groups.values() for number in numbers]
        if len(members) != len(set(members)):
            raise ValueError("A same-card member is listed twice")
        for keep, numbers in groups.items():
            for number in numbers:
                existing = self._existing_card(*number.split(":", 1))
                if existing is not None and existing != keep and existing in groups:
                    raise ValueError(
                        f"A same-card member belongs to another kept card: {number}"
                    )

    def _kept(self, previous: set[str]) -> str:
        """Several existing cards may only meet through one reviewed same-card group."""
        for keep, members in self.inputs.decisions.same_cards.items():
            folded = {
                card
                for member in members
                if (card := self._existing_card(*member.split(":", 1))) is not None
            }
            if keep in previous and previous - {keep} <= folded:
                for card in previous - {keep}:
                    self.merged[card] = keep
                return keep
        raise ValueError(
            "Grouping would merge permanent card IDs; list them in same_cards"
        )

    def _identity(self, group: list[tuple[str, Card]]) -> None:
        previous = {
            existing
            for region, item in group
            if (existing := self._existing_card(region, item.number)) is not None
        }
        region, first = group[0]
        card_id = (
            self._kept(previous)
            if len(previous) > 1
            else next(iter(previous))
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
                },
            )
            next_ids[region] += 1

    def curation(self) -> None:
        """Register confirmed absence, English art, reskins and source corrections."""
        only: dict[str, list[str]] = defaultdict(list)
        with_jp = {card for (region, _), card in self.parents.items() if region == "jp"}
        for number, target in self.inputs.mapping.targets.items():
            # An English-only printing folded into a JP card is no confirmed absence.
            if target is None and self.parents["en", number] not in with_jp:
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
                    "as_of": self.inputs.as_of.isoformat(),
                    "coverage_scope": "All Japanese official card extractions in the reviewed input batch",
                    "coverage_hash": self.inputs.jp_hash,
                    "observations": observations,
                },
            )
        self._arts()
        self._related()
        for correction in self.inputs.decisions.corrections:
            self._correction(correction)

    def _arts(self) -> None:
        groups = self.inputs.decisions.art_groups
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
                "expected_source_hash": digest(canonical(card.model_dump(mode="json"))),
                "source_hash_recipe": "registry-observation-v2",
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


def _repoint(entry: Entry, cards: dict[str, str], faces: dict[str, str]) -> Entry:
    """Move curation of a folded card onto the kept card's face of the same ordinal."""

    def move(value: JsonValue) -> JsonValue:
        if isinstance(value, str):
            return cards.get(value, faces.get(value, value))
        if isinstance(value, list):
            return [move(item) for item in value]
        if isinstance(value, dict):
            return {key: move(item) for key, item in value.items()}
        return value

    if not cards:
        return entry
    data = move(entry.data)
    assert isinstance(data, dict)
    return entry.model_copy(update={"data": data})


def build(inputs: Inputs, existing: dict[str, Entry]) -> list[Entry]:
    """Construct the complete candidate registry without filesystem mutations."""
    builder = Builder(inputs, existing)
    builder.identities()
    builder.allocations()
    builder.curation()
    # Later curation is independent of the initialization decisions; never drop it.
    faces = builder.folded_faces()
    for key, entry in existing.items():
        if entry.kind in {"art", "source_correction"}:
            builder.entries.setdefault(key, _repoint(entry, builder.merged, faces))
    return sorted(builder.entries.values(), key=lambda entry: entry.record_key)
