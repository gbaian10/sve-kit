"""Project one verified public snapshot into native facts and reference identities."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.core.json import array, canonical, digest, integer, object_value, string
from sve_carddb.export.text_owners import TextOwners

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.export.read_api import Snapshot
    from sve_carddb.export.reader import Row


@dataclass(frozen=True, repr=False)
class BattleInputs:
    """Native bytes and their identity-only bindings from one public manifest."""

    snapshot: bytes
    references: dict[str, JsonValue]


def build_battle_inputs(snapshot: Snapshot, region: str) -> BattleInputs:
    """Keep the entire region's catalogue independently of decks or selected programs."""
    try:
        return _build(snapshot, region)
    except ValueError, KeyError, TypeError:
        raise ValueError("Battle snapshot projection is unavailable") from None


class _Projection:
    def __init__(self, snapshot: Snapshot, region: str) -> None:
        self.region = region
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

    def face(self, printed: Row) -> Row:
        observations = list(map(object_value, array(printed["observations"])))
        versions = {
            string(row["revision_id"])
            for row in observations
            if row["state"] == "available"
        }
        if len(versions) != 1 or any(
            row["state"] != "available" for row in observations
        ):
            raise ValueError("Battle printing has no unique available observation")
        revision = self.owner(
            self.revisions[next(iter(versions))], string(printed["face_id"])
        )
        titles = array(revision["titles"])
        if len(titles) > 1:
            raise ValueError("Native title representation is incomplete")
        sections = self.sections(revision)
        if [row["ordinal"] for row in sections] != list(range(len(sections))):
            raise ValueError("Native sections require a complete ordinal sequence")
        if revision["class_code"] is None:
            raise ValueError("Native class requires a nonempty label")
        card_class = self.labels["class", string(revision["class_code"])]
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


def _build(snapshot: Snapshot, region: str) -> BattleInputs:
    if (
        region not in {"jp", "en"}
        or region not in array(snapshot.manifest["regions"])
        or re.fullmatch(r"sha256:[0-9a-f]{64}", snapshot.manifest_sha256) is None
    ):
        raise ValueError("Battle snapshot background mismatch")
    projection = _Projection(snapshot, region)
    records: dict[str, bytes] = {}
    faces: list[JsonValue] = []
    wording: list[JsonValue] = []
    for printing in sorted(
        snapshot.tables["printing"], key=lambda row: string(row["card_no"])
    ):
        if printing["region"] != region:
            continue
        if printing["card_no_state"] == "none" and printing["manual_type_code"] in {
            "ep",
            "sep",
        }:
            continue
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
        },
    )
