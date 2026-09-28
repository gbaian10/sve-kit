"""Cross-row constraints that JSON Schema cannot express."""

from typing import TYPE_CHECKING
from urllib.parse import urlsplit

from pydantic import JsonValue

from sve_carddb.snapshot.contract import definition
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    string,
)

if TYPE_CHECKING:
    from sve_carddb.snapshot.reader import Fragment, Row, View

_CARD_TABLES = {
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
_PRINT_TABLES = {"printing", "printing_product", "printing_image"}
_ART_TABLES = {"art", "digital_art_link"}


def _key(value: JsonValue) -> tuple[int, int | str]:
    if value is None:
        return (0, 0)
    if isinstance(value, str):
        return (1, value)
    return (2, integer(value))


def ordered_rows(rows: list[Row], fields: list[str]) -> None:
    """Enforce stable key order before positional joins."""
    keys = [tuple(_key(row[field]) for field in fields) for row in rows]
    if keys != sorted(set(keys)):
        raise ValueError("Rows must be sorted with unique keys")


def validate_fragments(fragments: list[Fragment]) -> None:
    """Validate sorting before any row_index can be interpreted."""
    for fragment in fragments:
        fields = (
            ["row_index"]
            if fragment.value["base"] is not None
            else [
                string(field)
                for field in array(definition(fragment.table)["x-primary-key"])
            ]
        )
        ordered_rows(fragment.rows, fields)
    for table in {fragment.table for fragment in fragments}:
        for file in {fragment.file for fragment in fragments}:
            values = [
                fragment
                for fragment in fragments
                if fragment.file == file and fragment.table == table
            ]
            keys = [
                (
                    string(object_value(f.value["owner"])["kind"]),
                    _key(object_value(f.value["owner"])["id"]),
                    integer(f.value["bucket"]),
                    string(f.value["partition"]),
                )
                for f in values
            ]
            if keys != sorted(keys):
                raise ValueError("Fragments are out of order")


def _parameter(value: Row) -> None:
    params = [object_value(item) for item in array(value["parameters"])]
    ordered_rows(params, ["name"])
    for item in params:
        if item["uint"] is not None:
            limits = object_value(item["uint"])
            if integer(limits["minimum"]) > integer(limits["maximum"]):
                raise ValueError("Inverted parameter range")


def _nested(value: JsonValue) -> None:
    if isinstance(value, list):
        for item in value:
            _nested(item)
    if not isinstance(value, dict):
        return
    if "parameters" in value:
        _parameter(value)
    for name, keys in [
        ("translations", ["field", "ordinal", "target_lang"]),
        ("sections", ["ordinal"]),
        ("regions", ["region"]),
        ("current", ["region"]),
        ("overrides", ["region"]),
        ("region_blocks", ["region"]),
    ]:
        if name in value and all(isinstance(item, dict) for item in array(value[name])):
            ordered_rows([object_value(item) for item in array(value[name])], keys)
    for item in value.values():
        _nested(item)


def _vocabulary(view: View) -> None:
    vocab = {(string(row["kind"]), string(row["code"])) for row in view["vocabulary"]}
    fields = {
        "class_code": "class",
        "type_code": "type",
        "rarity_code": "rarity",
        "frame_code": "frame",
        "series_code": "stamp_series",
        "traits": "trait",
        "titles": "title",
        "special_kinds": "special_kind",
    }

    def check(value: JsonValue) -> None:
        if isinstance(value, list):
            for item in value:
                check(item)
        if isinstance(value, dict):
            for field, item in value.items():
                if field in fields and item is not None:
                    values = item if isinstance(item, list) else [item]
                    if any(
                        (fields[field], string(code)) not in vocab for code in values
                    ):
                        raise ValueError("Vocabulary reference missing")
                check(item)

    for table in ("face_revision", "printing", "stamp"):
        for row in view[table]:
            check(row)


def _owner_card(
    table: str, row: Row, faces: dict[str, Row], arts: dict[str, Row]
) -> JsonValue:
    if table == "card":
        return row["id"]
    if table == "face_revision":
        return faces[string(row["face_id"])]["card_id"]
    if table == "digital_art_link":
        return arts[string(row["art_id"])]["card_id"]
    return row.get("card_id", row.get("from_card_id"))


def _owners(view: View, fragments: list[Fragment]) -> None:
    cards = {string(row["id"]): row for row in view["card"]}
    faces = {string(row["id"]): row for row in view["face"]}
    arts = {string(row["id"]): row for row in view["art"]}
    families = {row["id"] for row in view["product_family"]}
    print_owners: dict[str, JsonValue] = {}
    for fragment in fragments:
        owner = object_value(fragment.value["owner"])
        home = fragment.table in _CARD_TABLES | _PRINT_TABLES | _ART_TABLES
        if owner["kind"] != ("home_set" if home else "global") or (
            home and owner["id"] not in families
        ):
            raise ValueError("Incorrect owner kind or family")
        if fragment.value["base"] is not None:
            continue
        for row in fragment.rows:
            if fragment.table in _PRINT_TABLES:
                key = string(
                    row["id" if fragment.table == "printing" else "printing_id"]
                )
                if key in print_owners and print_owners[key] != owner["id"]:
                    raise ValueError("Printing fragments disagree on owner")
                print_owners[key] = owner["id"]
            elif fragment.table in _CARD_TABLES | _ART_TABLES:
                card_id = _owner_card(fragment.table, row, faces, arts)
                if cards[string(card_id)]["home_set_id"] != owner["id"]:
                    raise ValueError("Owner does not match card home_set")


def _summaries(view: View, manifest: Row) -> None:
    versions = {string(row["id"]): row for row in view["qa_version"]}
    qa = {
        string(card)
        for row in view["qa"]
        if row["current_version_id"] is not None
        for card in array(versions[string(row["current_version_id"])]["cards"])
    }
    faces = {string(row["id"]): string(row["card_id"]) for row in view["face"]}
    prints = {string(row["id"]): string(row["card_id"]) for row in view["printing"]}
    errata: set[str] = set()
    for row in view["errata"]:
        for raw in array(row["versions"]):
            version = object_value(raw)
            errata |= {
                faces[string(object_value(item)["face_id"])]
                for item in array(version["changes"])
            }
            errata |= {
                prints[string(object_value(item)["printing_id"])]
                for item in array(version["printings"])
            }
    if manifest["qa_card_ids"] != sorted(qa) or manifest["errata_card_ids"] != sorted(
        errata
    ):
        raise ValueError("QA/errata summary mismatch")
    universe: list[JsonValue] = [
        {key: row[key] for key in ("id", "kind", "definition_unit_id", "actions")}
        for row in view["keyword"]
    ]
    if digest(canonical(universe)) != manifest["mechanic_universe_id"]:
        raise ValueError("Mechanic universe hash mismatch")


def validate_view(view: View, manifest: Row, fragments: list[Fragment]) -> None:
    """Check public relationships without any build DB or source evidence."""
    _owners(view, fragments)
    _vocabulary(view)
    _summaries(view, manifest)
    for rows in view.values():
        for row in rows:
            _nested(row)
    if {row["card_id"] for row in view["card_engine_support"]} != {
        row["id"] for row in view["card"]
    }:
        raise ValueError("Every card requires support")
    images = {string(row["id"]): row for row in view["image_asset"]}
    for row in view["image_variant"]:
        asset = images[string(row["image_id"])]
        if (
            asset["publication_state"] != "approved"
            or asset["availability"] != "available"
        ):
            raise ValueError("Variant requires an approved available image")
    for row in view["printing_image"]:
        printing = next(p for p in view["printing"] if p["id"] == row["printing_id"])
        if row["face_id"] not in [
            object_value(f)["face_id"] for f in array(printing["faces"])
        ]:
            raise ValueError("Image references an absent printing face")


def validate_config(config: Row) -> None:
    """Verify language closure and the finite URL template parameter lists."""
    langs = [object_value(item) for item in array(config["languages"])]
    ordered_rows(langs, ["code"])
    codes = {string(item["code"]) for item in langs}
    for lang in langs:
        fallback = set(map(string, array(lang["fallback_order"])))
        if string(lang["code"]) in fallback or not fallback <= codes:
            raise ValueError("Invalid language fallback")
    for section, field, allowed in [
        ("digital_endpoints", "card_url_template", {"official_id", "provider_lang"}),
        ("shop_links", "url_template", {"card_no", "region"}),
    ]:
        for raw in array(config[section]):
            item = object_value(raw)
            template = string(item[field])
            for name in allowed:
                template = template.replace("{" + name + "}", "example")
            url = urlsplit(template)
            if url.username or url.password:
                raise ValueError("Public URL cannot contain credentials")
            if (
                "{" in template
                or "}" in template
                or url.scheme != "https"
                or not url.hostname
            ):
                raise ValueError("Invalid public URL template")
