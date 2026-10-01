"""Fixed owner and column partitions, derived only from verified logical rows."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.contract import columns, definition
from sve_carddb.snapshot.project.source import Source
from sve_carddb.snapshot.values import array, bucket, object_value, string

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build_db import Database
    from sve_carddb.snapshot.project import Projection

type Record = dict[str, JsonValue]
type View = dict[str, list[Record]]

CARD_TABLES = frozenset(
    {
        "card",
        "face",
        "face_revision",
        "card_engine_support",
        "mechanic_projection",
        "card_mechanic_coverage",
        "card_related",
        "digital_link",
        "digital_link_coverage",
        "card_voice",
    }
)
PRINT_TABLES = frozenset({"printing", "printing_product", "printing_image"})
ART_TABLES = frozenset({"art", "digital_art_link"})
IMAGE_TABLES = frozenset({"image_asset", "printing_image", "image_variant"})


@dataclass(frozen=True, order=True)
class Group:
    """One deterministic file group; global IDs use an empty internal sentinel."""

    role: str
    partition: str
    kind: str
    identifier: str
    bucket: int

    @property
    def owner(self) -> Record:
        """Encode the required nullable global owner ID."""
        return {"kind": self.kind, "id": self.identifier or None}


@dataclass(frozen=True)
class Ownership:
    """Build-only printing ownership, never added to public rows."""

    printing_home: Mapping[str, str]

    @classmethod
    def from_database(cls, db: Database, projection: Projection) -> Ownership:
        """Read exact registered homes through the typed SQLite boundary."""
        public = {row["id"] for row in projection.tables["printing"]}
        return cls(
            {
                string(row["id"]): string(row["home_set_id"])
                for row in Source(db).rows("printing", "id,home_set_id")
                if row["id"] in public
            }
        )


class Layout:
    """Indexes for fixed partitioning; no nearest-reference owner inference."""

    def __init__(self, view: View, ownership: Ownership, count: int) -> None:
        self.view = view
        self.count = count
        self.ownership = ownership
        self.cards = {string(row["id"]): row for row in view["card"]}
        self.faces = {string(row["id"]): row for row in view["face"]}
        self.arts = {string(row["id"]): row for row in view["art"]}
        self.display = {
            string(object_value(item)["revision_id"])
            for row in view["face"]
            for item in array(row["current"])
        } | {
            string(display["revision_id"])
            for row in view["face"]
            for raw in array(row["wording"])
            for display in (object_value(object_value(raw)["display"]),)
            if display["revision_id"] is not None
        }
        if set(ownership.printing_home) != {row["id"] for row in view["printing"]}:
            raise ValueError("Exact public printing ownership required")
        homes = {row["id"] for row in view["product_family"]}
        if not set(ownership.printing_home.values()) <= homes:
            raise ValueError("Printing home must be a public product family")

    def group(self, table: str, row: Record, partition: str) -> Group:
        """Hash the complete primary entity key and use its registered home."""
        identifier = ""
        if table in PRINT_TABLES:
            entity = string(row["id" if table == "printing" else "printing_id"])
            identifier = self.ownership.printing_home[entity]
            key: list[JsonValue] = [entity]
        elif table in ART_TABLES:
            entity = string(row["id" if table == "art" else "art_id"])
            identifier = string(
                self.cards[string(self.arts[entity]["card_id"])]["home_set_id"]
            )
            key = [entity]
        elif table in CARD_TABLES:
            entity = string(
                row["id"]
                if table == "card"
                else self.faces[string(row["face_id"])]["card_id"]
                if table == "face_revision"
                else row["from_card_id"]
                if table == "card_related"
                else row["card_id"]
            )
            identifier = string(self.cards[entity]["home_set_id"])
            key = [entity]
        else:
            key = [
                row[string(field)]
                for field in array(definition(table)["x-primary-key"])
            ]
        role = (
            "images"
            if table in IMAGE_TABLES
            else "bootstrap"
            if partition == "bootstrap"
            else "text"
        )
        return Group(
            role,
            partition,
            "home_set" if identifier else "global",
            identifier,
            bucket(key, self.count),
        )

    def record(self, table: str, row: Record, partition: str, index: int) -> Record:
        """Split each public value exactly once, including permanent face ordinals."""
        if table == "printing":
            if partition == "bootstrap":
                return row | {
                    "faces": [
                        {
                            field: object_value(item)[field]
                            for field in columns("PrintingFaceBootstrap")
                        }
                        for item in array(row["faces"])
                    ]
                }
            return {
                "row_index": index,
                "faces": [
                    {
                        "face_ordinal": self.faces[
                            string(object_value(item)["face_id"])
                        ]["ordinal"]
                    }
                    | {
                        field: object_value(item)[field]
                        for field in columns("PrintingFaceDetail")
                        if field != "face_ordinal"
                    }
                    for item in array(row["faces"])
                ],
            }
        if table == "face_revision" and partition != "history":
            names = columns("face_revision_" + partition)
            return (
                {field: row[field] for field in names if field != "row_index"}
                | {
                    "translations": [
                        item
                        for item in array(row["translations"])
                        if (object_value(item)["field"] == "name")
                        == (partition == "bootstrap")
                    ]
                }
                | ({"row_index": index} if partition == "detail" else {})
            )
        return row.copy()


def references(value: JsonValue) -> tuple[set[str], set[str]]:
    """Find bootstrap text/translation closure without following optional history."""
    texts: set[str] = set()
    translations: set[str] = set()
    if isinstance(value, dict):
        for field, item in value.items():
            if item is not None and field.endswith("_unit_id"):
                texts.add(string(item))
            if item is not None and field == "translation_id":
                translations.add(string(item))
            nested_texts, nested_translations = references(item)
            texts |= nested_texts
            translations |= nested_translations
    elif isinstance(value, list):
        for item in value:
            nested_texts, nested_translations = references(item)
            texts |= nested_texts
            translations |= nested_translations
    return texts, translations
