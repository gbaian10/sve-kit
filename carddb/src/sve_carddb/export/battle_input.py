"""Project one verified public snapshot into native facts and reference identities."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.contracts.profiles import PROGRAM_DEPENDENCIES
from sve_carddb.core.json import array, canonical, digest, integer, object_value, string
from sve_carddb.export.battle_rules import rule_identities
from sve_carddb.export.text_owners import TextOwners

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.export.read_api import Snapshot
    from sve_carddb.export.reader import Row


# Printed EP/SEP resources: classless, and manual copies never become native cards.
RESOURCE_TYPES = frozenset({"ep", "sep"})


@dataclass(frozen=True, repr=False)
class BattleInputs:
    """Native bytes and their identity-only bindings from one public manifest."""

    snapshot: bytes
    references: dict[str, JsonValue]
    programs: ProgramInputs | None = None


@dataclass(frozen=True, repr=False)
class ProgramInputs:
    """Exact program attachment and its native printing and rule projections."""

    descriptor: Row
    printings: list[JsonValue]
    rule_faces: dict[tuple[str, int], Row]


def build_battle_inputs(
    snapshot: Snapshot, region: str, *, published: bool = False
) -> BattleInputs:
    """Keep the entire region's catalogue independently of decks or selected programs."""
    try:
        return _build(snapshot, region, published=published)
    except ValueError, KeyError, TypeError:
        raise ValueError("Battle snapshot projection is unavailable") from None


class _Projection:
    def __init__(self, snapshot: Snapshot, region: str, *, published: bool) -> None:
        self.region = region
        self.published = published
        self.owners = TextOwners(snapshot.tables)
        self.texts = {
            string(row["id"]): string(row["text"])
            for row in snapshot.tables["text_unit"]
        }
        self.labels = {
            (string(row["kind"]), string(row["code"])): self.texts[
                string(row["label_unit_id"])
            ]
            for row in snapshot.tables["vocabulary"]
        }
        self.revisions = {
            string(row["id"]): row for row in snapshot.tables["face_revision"]
        }
        self.face_revisions: dict[str, list[Row]] = {}
        for revision in sorted(
            self.revisions.values(), key=lambda row: string(row["id"])
        ):
            if revision["region"] == region:
                self.face_revisions.setdefault(string(revision["face_id"]), []).append(
                    revision
                )
        self.faces = {string(row["id"]): row for row in snapshot.tables["face"]}
        self.cards = {string(row["id"]): row for row in snapshot.tables["card"]}

    def owner(self, revision: Row, face: str) -> Row:
        owner = self.owners.get({"kind": "face_revision", "id": revision["id"]})
        if owner.region != self.region or owner.face_id != face:
            raise ValueError("Battle wording owner mismatch")
        return revision

    def text(self, revision: Row, field: str, ordinal: int | None = None) -> str:
        owner = self.owners.get({"kind": "face_revision", "id": revision["id"]})
        return self.texts[owner.field(field, ordinal)]

    @staticmethod
    def sections(revision: Row) -> list[Row]:
        sections = sorted(
            map(object_value, array(revision["sections"])),
            key=lambda row: integer(row["ordinal"]),
        )
        ordinals = [integer(row["ordinal"]) for row in sections]
        if any(ordinal < 0 for ordinal in ordinals) or len(set(ordinals)) != len(
            ordinals
        ):
            raise ValueError("Wording section ordinals must be unique and nonnegative")
        return sections

    def current(self, face: str) -> Row:
        candidates = [
            string(current["revision_id"])
            for current in map(object_value, array(self.faces[face]["current"]))
            if current["region"] == self.region
        ]
        if len(candidates) != 1:
            raise ValueError("Battle face has no unique adopted revision")
        return self.owner(self.revisions[candidates[0]], face)

    def face(self, printed: Row) -> Row:
        observations = list(map(object_value, array(printed["observations"])))
        versions = {
            string(row["revision_id"])
            for row in observations
            if row["state"] == "available"
        }
        face = string(printed["face_id"])
        if (
            self.published
            and observations
            and all(row["state"] == "missing_effect" for row in observations)
        ):
            revision = self.current(face)
        elif len(versions) == 1 and all(
            row["state"] == "available" for row in observations
        ):
            revision = self.owner(self.revisions[next(iter(versions))], face)
        else:
            raise ValueError("Battle printing has no unique available observation")
        titles = array(revision["titles"])
        if len(titles) > 1:
            raise ValueError("Native title representation is incomplete")
        sections = self.sections(revision)
        if [row["ordinal"] for row in sections] != list(range(len(sections))):
            raise ValueError("Native sections require a complete ordinal sequence")
        if revision["class_code"] is not None:
            card_class = self.labels["class", string(revision["class_code"])]
        elif revision["type_code"] in RESOURCE_TYPES:
            # EP/SEP have no class; the native format writes absent characteristics as "-".
            card_class = "-"
        else:
            raise ValueError("Native class requires a nonempty label")
        if not card_class:
            raise ValueError("Native class requires a nonempty label")
        result: Row = {
            "name": self.text(revision, "name"),
            "card_class": card_class,
            "card_type": "・".join(
                [
                    self.labels["type", string(revision["type_code"])],
                    *[
                        self.labels["special_kind", string(kind)]
                        for kind in array(revision["special_kinds"])
                    ],
                ]
            ),
            "traits": [
                self.labels["trait", string(trait)]
                for trait in array(revision["traits"])
            ],
            "text": None
            if revision["effect_unit_id"] is None
            else self.text(revision, "effect"),
            "sections": [
                self.text(revision, "section", integer(section["ordinal"]))
                for section in sections
            ],
        }
        for target, field in [("cost", "cost"), ("power", "attack"), ("hp", "defense")]:
            result[target] = (
                "-" if revision[field] is None else str(integer(revision[field]))
            )
        if titles:
            result["title"] = self.labels["title", string(titles[0])]
        return result

    def wording(self, face: str) -> list[JsonValue]:
        result: list[JsonValue] = []
        for revision in self.face_revisions.get(face, []):
            self.owner(revision, face)
            fields: list[JsonValue] = []
            if revision["effect_unit_id"] is not None:
                self.text(revision, "effect")
                fields.append({"kind": "effect"})
            for section in self.sections(revision):
                ordinal = integer(section["ordinal"])
                self.text(revision, "section", ordinal)
                fields.append({"kind": "section", "ordinal": ordinal})
            result.append({"revision_id": revision["id"], "fields": fields})
        return result

    def printing_faces(self, printing: Row) -> list[Row]:
        faces = sorted(
            map(object_value, array(printing["faces"])),
            key=lambda row: integer(self.faces[string(row["face_id"])]["ordinal"]),
        )
        expected = array(self.cards[string(printing["card_id"])]["faces"])
        if [row["face_id"] for row in faces] != expected or not faces:
            raise ValueError("Battle printing face closure is incomplete")
        if any(
            self.faces[string(row["face_id"])]["ordinal"] != ordinal
            for ordinal, row in enumerate(faces)
        ):
            raise ValueError("Native faces require a complete ordinal sequence")
        return faces


def _program_inputs(
    snapshot: Snapshot, projection: _Projection, printings: list[Row]
) -> ProgramInputs:
    descriptors = [
        row
        for row in map(object_value, array(snapshot.manifest["files"]))
        if row["role"] == "programs"
    ]
    if len(descriptors) != 1:
        raise ValueError("Battle program attachment is missing or ambiguous")
    names = {
        string(row["id"]): string(row["official_name"])
        for row in snapshot.tables["rules_name"]
        if row["region"] == "jp"
    }
    primary: dict[str, list[str]] = {}
    for row in snapshot.tables["face_rules_name"]:
        if row["region"] == "jp" and row["role"] == "primary":
            primary.setdefault(string(row["face_id"]), []).append(
                string(row["rules_name_id"])
            )
    rule_faces: dict[tuple[str, int], Row] = {}
    projected: list[JsonValue] = []
    for printing in printings:
        number = string(printing["card_no"])
        faces: list[JsonValue] = []
        for ordinal, face in enumerate(projection.printing_faces(printing)):
            face_id = string(face["face_id"])
            current = projection.current(face_id)
            named = primary.get(face_id, [])
            name_id = named[0] if len(named) == 1 else None
            titles = array(current["titles"])
            faces.append(
                {
                    "face_id": face_id,
                    "revision_id": current["id"],
                    "cost": current["cost"],
                    "rules_name": names[name_id] if name_id is not None else None,
                }
            )
            rule_faces[number, ordinal] = {
                "card_no": number,
                "face_ordinal": ordinal,
                "face_id": face_id,
                "rules_name_id": name_id,
                "title_code": titles[0] if titles else None,
            }
        projected.append(
            {
                "printing_id": printing["id"],
                "card_no": number,
                "card_id": printing["card_id"],
                "faces": faces,
            }
        )
    return ProgramInputs(descriptors[0], projected, rule_faces)


def _official_printings(snapshot: Snapshot, region: str) -> list[Row]:
    printings = [
        printing
        for printing in snapshot.tables["printing"]
        if printing["region"] == region
        and not (
            printing["card_no_state"] == "none"
            and printing["manual_type_code"] in RESOURCE_TYPES
        )
    ]
    return sorted(printings, key=lambda row: string(row["card_no"]))


def _build(snapshot: Snapshot, region: str, *, published: bool = False) -> BattleInputs:
    if (
        region not in {"jp", "en"}
        or region not in array(snapshot.manifest["regions"])
        or re.fullmatch(r"sha256:[0-9a-f]{64}", snapshot.manifest_sha256) is None
    ):
        raise ValueError("Battle snapshot background mismatch")
    if published and (
        region != "jp" or snapshot.manifest["format_version"] != PROGRAM_DEPENDENCIES
    ):
        raise ValueError("Published battle inputs require a 4.2.0 JP snapshot")
    projection = _Projection(snapshot, region, published=published)
    records: dict[str, bytes] = {}
    faces: list[JsonValue] = []
    wording: list[JsonValue] = []
    printings = _official_printings(snapshot, region)
    for printing in printings:
        number = string(printing["card_no"])
        if printing["card_no_state"] != "official" or not number or number in records:
            raise ValueError("Battle printing locator is unavailable or ambiguous")
        native: list[JsonValue] = []
        for ordinal, face in enumerate(projection.printing_faces(printing)):
            native.append(projection.face(face))
            faces.append(
                {
                    "card_no": number,
                    "native_face": ordinal,
                    "card": {"printing_id": printing["id"], "face": ordinal},
                }
            )
            wording.append(
                {
                    "card_no": number,
                    "native_face": ordinal,
                    "face_id": face["face_id"],
                    "revisions": projection.wording(string(face["face_id"])),
                }
            )
        records[number] = canonical({"number": number, "faces": native})
    if not records:
        raise ValueError("Battle catalogue is empty")
    native_snapshot = b"\n".join(records.values()) + b"\n"
    return BattleInputs(
        native_snapshot,
        {
            "region": region,
            "snapshot_sha256": digest(native_snapshot).removeprefix("sha256:"),
            "faces": faces,
            "wording": {"manifest_sha256": snapshot.manifest_sha256, "faces": wording},
            "rules": rule_identities(snapshot, region),
        },
        _program_inputs(snapshot, projection, printings) if published else None,
    )
