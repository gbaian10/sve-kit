"""Region filtering and public reachability without exposing build provenance."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import array, digest, integer, object_value, string
from sve_carddb.export.references import references

if TYPE_CHECKING:
    from sve_carddb.export.project.source import Record


def select_regions(view: dict[str, list[Record]], regions: tuple[str, ...]) -> None:
    """Retain selected identities plus tombstones needed by public repair events."""
    view["printing"] = [row for row in view["printing"] if row["region"] in regions]
    cards = {string(row["card_id"]) for row in view["printing"]}
    while True:
        before = set(cards)
        for change in view["identity_change"]:
            if change["new_card_id"] in cards or change["old_card_id"] in cards:
                cards.update(
                    (string(change["old_card_id"]), string(change["new_card_id"]))
                )
        if cards == before:
            break
    view["identity_change"] = [
        row
        for row in view["identity_change"]
        if row["old_card_id"] in cards and row["new_card_id"] in cards
    ]
    view["card"] = [row for row in view["card"] if row["id"] in cards]
    for table in (
        "face",
        "digital_link",
        "card_voice",
        "digital_link_coverage",
        "card_mechanic_coverage",
        "mechanic_projection",
    ):
        view[table] = [row for row in view[table] if row["card_id"] in cards]
    faces = {row["id"] for row in view["face"]}
    prints = {row["id"] for row in view["printing"]}
    for table in ("face_revision", "face_rules_name"):
        view[table] = [
            row
            for row in view[table]
            if row["face_id"] in faces and row["region"] in regions
        ]
    view["printing_product"] = [
        row for row in view["printing_product"] if row["printing_id"] in prints
    ]
    for table in (
        "product",
        "qa",
        "cr_version",
        "rules_profile",
        "rules_name",
        "errata",
    ):
        view[table] = [row for row in view[table] if row["region"] in regions]
    products = {row["product_id"] for row in view["printing_product"]}
    view["product"] = [row for row in view["product"] if row["id"] in products]
    for table, field, parent in (
        ("qa_version", "qa_id", "qa"),
        ("cr_clause", "cr_version_id", "cr_version"),
        ("restriction", "profile_id", "rules_profile"),
    ):
        identifiers = {row["id"] for row in view[parent]}
        view[table] = [row for row in view[table] if row[field] in identifiers]
    view["card_related"] = [
        row
        for row in view["card_related"]
        if row["from_card_id"] in cards and row["to_card_id"] in cards
    ]


def prune(view: dict[str, list[Record]]) -> None:
    """Remove unused translations/text/digital assets and filtered art references."""
    art_ids = {row["id"] for row in view["art"]}
    for printing in view["printing"]:
        for raw in array(printing["faces"]):
            face = object_value(raw)
            if face["art_id"] not in art_ids:
                face["art_id"] = None
    view["digital_art_link"] = [
        row for row in view["digital_art_link"] if row["art_id"] in art_ids
    ]
    scoped: dict[str, set[str]] = {}
    for rows in view.values():
        for row in rows:
            for table, identifier in references(row):
                scoped.setdefault(table, set()).add(identifier)
    for table in ("stamp", "digital_art", "voice"):
        view[table] = [
            row for row in view[table] if row["id"] in scoped.get(table, set())
        ]
    digital = {
        row["digital_card_id"]
        for table in ("digital_art", "digital_link", "voice")
        for row in view[table]
    }
    digital.update(
        row["interaction_target_id"]
        for row in view["voice"]
        if row["interaction_target_id"] is not None
    )
    view["digital_card"] = [row for row in view["digital_card"] if row["id"] in digital]
    text_ids = {
        identifier
        for table, rows in view.items()
        if table != "text_unit"
        for row in rows
        for target, identifier in references(row)
        if target == "text_unit"
    }
    view["text_unit"] = [row for row in view["text_unit"] if row["id"] in text_ids]


def validate_closure(view: dict[str, list[Record]]) -> None:
    """Fail on any unresolved public ID; never fill a hole from another snapshot."""
    targets = {
        table: {string(row["id"]) for row in rows if "id" in row}
        for table, rows in view.items()
    }
    for rows in view.values():
        for row in rows:
            for table, identifier in references(row):
                if identifier not in targets[table]:
                    raise ValueError(f"Dangling public reference to {table}")


def validate_identities(view: dict[str, list[Record]]) -> None:
    """Check public face ownership and exact text IDs after capability filtering."""
    faces = {row["id"]: row for row in view["face"]}
    for card in view["card"]:
        actual = sorted(
            [row for row in view["face"] if row["card_id"] == card["id"]],
            key=lambda row: integer(row["ordinal"]),
        )
        if card["faces"] != [row["id"] for row in actual]:
            raise ValueError("Card face closure mismatch")
        if card["identity_state"] != "retired":
            expected = ["front"] if card["layout"] == "single" else ["front", "back"]
            if [row["side"] for row in actual] != expected or [
                row["ordinal"] for row in actual
            ] != list(range(len(expected))):
                raise ValueError("Public card layout/face coverage mismatch")
    for printing in view["printing"]:
        for raw in array(printing["faces"]):
            face = object_value(raw)
            if faces[face["face_id"]]["card_id"] != printing["card_id"]:
                raise ValueError("Printing face crosses card")
    for text in view["text_unit"]:
        expected_id = (
            "t:"
            + string(text["lang"])
            + ":"
            + digest(string(text["text"]).encode())[7:23]
        )
        if text["id"] != expected_id:
            raise ValueError("Text ID does not match exact text")
