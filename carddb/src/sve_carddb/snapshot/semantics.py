"""Cross-row constraints that JSON Schema cannot express."""

from typing import TYPE_CHECKING
from urllib.parse import quote, urlsplit

from pydantic import JsonValue

from sve_carddb.snapshot.contract import definition
from sve_carddb.snapshot.profiles import profile
from sve_carddb.snapshot.values import (
    array,
    bucket,
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


def _spellings(symbol: Row) -> None:
    params = {
        string(object_value(item)["name"]): object_value(item)
        for item in array(object_value(symbol["parameter_schema"])["parameters"])
    }
    for raw in array(symbol["spellings"]):
        spelling = object_value(raw)
        kind = spelling["parse_kind"]
        if kind == "literal":
            continue
        name = string(spelling["parameter_name"])
        if name not in params:
            raise ValueError("Spelling references an undeclared parameter")
        parameter = params[name]
        if (kind == "uint" and parameter["uint"] is None) or (
            kind == "variable" and not array(parameter["variables"])
        ):
            raise ValueError("Spelling requires an enabled parameter domain")


def _hints(ruling: Row) -> None:
    schemas = {
        canonical(object_value(hint)["parameter_schema"])
        for hint in array(ruling["hints"])
    }
    if len(schemas) > 1:
        raise ValueError("Multilingual hints must declare identical parameters")


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
        ("wording", ["region"]),
        ("candidates", ["printing_id", "revision_id"]),
        ("observations", ["source_url", "state", "revision_id"]),
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
    for symbol in view["text_symbol"]:
        _spellings(symbol)
    for ruling in view["ruling_revision"]:
        _hints(ruling)
    _wording(view)
    if {row["card_id"] for row in view["card_engine_support"]} != {
        row["id"] for row in view["card"]
    }:
        raise ValueError("Every card requires support")
    active = {
        string(row["image_id"])
        for row in view["printing_image"]
        if row["publication_state"] == "approved" and row["availability"] == "available"
    }
    for row in view["image_variant"]:
        if string(row["image_id"]) not in active:
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


def _physical_observations(
    view: View, revisions: dict[str, Row]
) -> dict[tuple[str, str], tuple[str, Row]]:
    physical: dict[tuple[str, str], tuple[str, Row]] = {}
    for printing in view["printing"]:
        for raw in array(printing["faces"]):
            face = object_value(raw)
            physical[string(printing["id"]), string(face["face_id"])] = (
                string(printing["region"]),
                face,
            )
            for raw_observation in array(face["observations"]):
                observation = object_value(raw_observation)
                if observation["revision_id"] is not None:
                    revision = revisions[string(observation["revision_id"])]
                    if (
                        revision["face_id"] != face["face_id"]
                        or revision["region"] != printing["region"]
                    ):
                        raise ValueError(
                            "Observation revision belongs to another face/region"
                        )
    return physical


def _wording_dates(view: View) -> dict[str, str | None]:
    products = {string(row["id"]): row for row in view["product"]}
    dates: dict[str, list[str | None]] = {
        string(row["id"]): [] for row in view["printing"]
    }
    for inclusion in view["printing_product"]:
        product = products[string(inclusion["product_id"])]
        precision = inclusion["date_precision"]
        day = inclusion["available_on"]
        if precision is None:
            precision, day = product["date_precision"], product["released_on"]
        dates[string(inclusion["printing_id"])].append(
            string(day) if precision == "day" else None
        )
    return {
        printing: min(day for day in items if day is not None)
        if items and all(day is not None for day in items)
        else None
        for printing, items in dates.items()
    }


def _wording_candidates(
    face: Row,
    wording: Row,
    physical: dict[tuple[str, str], tuple[str, Row]],
    dates: dict[str, str | None],
    revisions: dict[str, Row],
) -> None:
    candidates = [object_value(item) for item in array(wording["candidates"])]
    printings = {string(item["printing_id"]) for item in candidates}
    undated = array(wording["undated_printing_ids"])
    if undated != sorted(printing for printing in printings if dates[printing] is None):
        raise ValueError("Invalid undated wording printing inventory")
    for candidate in candidates:
        parent = physical.get((string(candidate["printing_id"]), string(face["id"])))
        if parent is None or parent[0] != wording["region"]:
            raise ValueError(
                "Wording candidate belongs to another printing face/region"
            )
        observed = [
            object_value(item)["revision_id"]
            for item in array(parent[1]["observations"])
        ]
        if candidate["revision_id"] not in (observed or [None]):
            raise ValueError("Wording candidate has no printing observation")
    display = object_value(wording["display"])
    if display["basis"] == "latest_known_release":
        known = {dates[printing] for printing in printings} - {None}
        if not known:
            raise ValueError("Latest wording display requires a complete release day")
        latest = max(day for day in known if day is not None)
        newest = [
            item for item in candidates if dates[string(item["printing_id"])] == latest
        ]
        if (
            len(
                {
                    canonical(
                        {
                            field: revisions[string(item["revision_id"])][field]
                            for field in (
                                "name_unit_id",
                                "effect_unit_id",
                                "class_code",
                                "type_code",
                                "cost",
                                "attack",
                                "defense",
                                "traits",
                                "titles",
                                "special_kinds",
                                "sections",
                            )
                        }
                    )
                    for item in newest
                    if item["revision_id"] is not None
                }
            )
            != 1
            or any(item["revision_id"] is None for item in newest)
            or any(
                object_value(observation)["state"] == "correction_conflict"
                for item in newest
                for observation in array(
                    physical[string(item["printing_id"]), string(face["id"])][1][
                        "observations"
                    ]
                )
            )
            or display["revision_id"] not in [item["revision_id"] for item in newest]
        ):
            raise ValueError(
                "Latest wording display skips an unavailable/latest printing"
            )


def _wording_display(
    face: Row,
    wording: Row,
    revisions: dict[str, Row],
    current: dict[str, JsonValue],
    blocks: dict[str, list[JsonValue]],
) -> None:
    region = string(wording["region"])
    display = object_value(wording["display"])
    if display["basis"] == "current":
        if display["revision_id"] != current.get(region):
            raise ValueError("Pending current display disagrees with adopted current")
    else:
        if region in current:
            raise ValueError("Pending display must preserve a valid current")
        if "wording_pending" not in blocks.get(region, []):
            raise ValueError("Missing wording_pending region block")
        if display["revision_id"] is not None and display["revision_id"] not in [
            object_value(item)["revision_id"] for item in array(wording["candidates"])
        ]:
            raise ValueError("Display is not a pending candidate")
    if display["revision_id"] is not None:
        revision = revisions[string(display["revision_id"])]
        if revision["face_id"] != face["id"] or revision["region"] != region:
            raise ValueError("Display belongs to another face/region")


def _wording(view: View) -> None:
    revisions = {string(row["id"]): row for row in view["face_revision"]}
    physical = _physical_observations(view, revisions)
    dates = _wording_dates(view)
    blocks = {
        string(row["card_id"]): {
            string(object_value(item)["region"]): array(object_value(item)["reasons"])
            for item in array(row["region_blocks"])
        }
        for row in view["card_engine_support"]
    }
    for face in view["face"]:
        current = {
            string(object_value(item)["region"]): object_value(item)["revision_id"]
            for item in array(face["current"])
        }
        pending = {
            string(object_value(item)["region"]): object_value(item)
            for item in array(face["wording"])
        }
        required = {
            region
            for (_, physical_face), (region, _) in physical.items()
            if physical_face == face["id"]
        }
        if (
            not required <= current.keys() | pending.keys()
            or not pending.keys() <= required
        ):
            raise ValueError(
                "Face without current requires pending wording in every region"
            )
        for wording in pending.values():
            _wording_display(
                face,
                wording,
                revisions,
                current,
                blocks.get(string(face["card_id"]), {}),
            )
            _wording_candidates(face, wording, physical, dates, revisions)


def validate_placement(view: View, manifest: Row, fragments: list[Fragment]) -> None:
    """Recompute entity buckets from joined identities, independently of the exporter."""
    selected = profile(string(manifest["format_version"]))
    faces = {string(row["id"]): row for row in view["face"]}
    bases = {
        (
            f.file,
            f.table,
            canonical(f.value["owner"]),
            integer(f.value["bucket"]),
        ): f.rows
        for f in fragments
        if f.value["partition"] == "bootstrap"
    }
    for fragment in fragments:
        owner = object_value(fragment.value["owner"])
        kind = string(owner["kind"])
        identifier = string(owner["id"]) if owner["id"] is not None else ""
        part = string(fragment.value["partition"])
        role = (
            "images"
            if fragment.table in {"image_asset", "image_variant", "printing_image"}
            else "bootstrap"
            if part == "bootstrap"
            else "text"
        )
        width = selected.width(role, part, kind, identifier)
        band = integer(fragment.value["bucket"]) // width
        key = "/".join(
            (
                role,
                part,
                kind,
                quote(identifier, safe="") or "global",
                "band",
                str(band),
            )
        )
        if fragment.file != key:
            raise ValueError("File key does not match fixed band recipe")
        rows = fragment.rows
        if fragment.value["base"] is not None:
            base = object_value(fragment.value["base"])
            rows = bases[
                string(object_value(base["file"])["key"]),
                fragment.table,
                canonical(base["owner"]),
                integer(base["bucket"]),
            ]
        for row in rows:
            key_values = _entity_key(fragment.table, row, faces)
            if bucket(key_values, selected.buckets) != fragment.value["bucket"]:
                raise ValueError("Fragment entity bucket does not match fixed profile")


def _entity_key(table: str, row: Row, faces: dict[str, Row]) -> list[JsonValue]:
    if table in _CARD_TABLES:
        entity = (
            row["id"]
            if table == "card"
            else faces[string(row["face_id"])]["card_id"]
            if table == "face_revision"
            else row["from_card_id"]
            if table == "card_related"
            else row["card_id"]
        )
        return [entity]
    if table in _PRINT_TABLES:
        return [row["id" if table == "printing" else "printing_id"]]
    if table in _ART_TABLES:
        return [row["id" if table == "art" else "art_id"]]
    if table in {"image_asset", "image_variant"}:
        return [row["id" if table == "image_asset" else "image_id"]]
    return [row[string(field)] for field in array(definition(table)["x-primary-key"])]
