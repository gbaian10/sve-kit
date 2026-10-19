"""Cross-row constraints that JSON Schema cannot express."""

from dataclasses import dataclass
from typing import TYPE_CHECKING
from urllib.parse import quote, urlsplit

from pydantic import JsonValue

from sve_carddb.contracts.profiles import profile
from sve_carddb.contracts.snapshot import definition
from sve_carddb.core.json import array, canonical, digest, integer, object_value, string
from sve_carddb.core.wording import Wording, preferred_wordings
from sve_carddb.export.buckets import bucket
from sve_carddb.export.common_names import common_name
from sve_carddb.export.name_annotations import whole_name
from sve_carddb.export.reader_annotations import validate_annotations
from sve_carddb.export.text_owners import row_owners

if TYPE_CHECKING:
    from sve_carddb.contracts.profiles import Profile
    from sve_carddb.export.reader import Fragment, Row, View

_CARD_TABLES = {
    "card",
    "face",
    "face_revision",
    "card_engine_support",
    "mechanic_projection",
    "card_mechanic_coverage",
    "card_related",
    "card_mentioned_by",
    "digital_link",
    "digital_link_coverage",
    "card_voice",
}
_PRINT_TABLES = {"printing", "printing_product", "printing_image"}
_ART_TABLES = {"art", "digital_art_link"}


def row_key(value: JsonValue) -> tuple[int, int | str | bytes]:
    """Sort structured owner keys by canonical bytes without comparing mappings."""
    if value is None:
        return (0, 0)
    if isinstance(value, str):
        return (1, value)
    if isinstance(value, (dict, list)):
        return (3, canonical(value))
    return (2, integer(value))


def ordered_rows(rows: list[Row], fields: list[str]) -> None:
    """Enforce stable key order before positional joins."""
    keys = [tuple(row_key(row[field]) for field in fields) for row in rows]
    if keys != sorted(set(keys)):
        raise ValueError("Rows must be sorted with unique keys")


def validate_fragments(fragments: list[Fragment]) -> None:
    """Validate sorting before any row_index can be interpreted."""
    groups: dict[tuple[str, str], list[Fragment]] = {}
    for fragment in fragments:
        if fragment.table == "__catalog":
            continue
        groups.setdefault((fragment.table, fragment.file), []).append(fragment)
        fields = (
            ["row_index"]
            if fragment.value["base"] is not None
            else [
                string(field)
                for field in array(definition(fragment.table)["x-primary-key"])
            ]
        )
        ordered_rows(fragment.rows, fields)
    for values in groups.values():
        keys = [
            (
                string(object_value(f.value["owner"])["kind"]),
                row_key(object_value(f.value["owner"])["id"]),
                string(f.value["segment"]),
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
        if fragment.table == "field_annotation":
            continue
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
    _annotation_owners(view, fragments, print_owners)
    _annotation_partitions(view, fragments)


def _annotation_owners(
    view: View, fragments: list[Fragment], print_owners: dict[str, JsonValue]
) -> None:
    cards = {string(row["id"]): row for row in view["card"]}
    faces = {string(row["id"]): row for row in view["face"]}
    revisions = {string(row["id"]): row for row in view["face_revision"]}
    for fragment in fragments:
        if fragment.table != "field_annotation":
            continue
        location = object_value(fragment.value["owner"])
        for row in fragment.rows:
            owner = object_value(row["owner"])
            expected: JsonValue = None
            if owner["kind"] == "face_revision":
                revision = revisions[string(owner["id"])]
                card_id = faces[string(revision["face_id"])]["card_id"]
                expected = cards[string(card_id)]["home_set_id"]
            elif owner["kind"] == "printing_face":
                expected = print_owners[string(owner["id"])]
            if location != {
                "kind": "global" if expected is None else "home_set",
                "id": expected,
            }:
                raise ValueError(
                    "Original annotation fragment owner differs from source owner"
                )


def _annotation_partitions(view: View, fragments: list[Fragment]) -> None:
    display = (
        {
            string(object_value(item)["revision_id"])
            for face in view["face"]
            for item in array(face["current"])
        }
        | {
            string(value["revision_id"])
            for face in view["face"]
            for raw in array(face["wording"])
            for value in (object_value(object_value(raw)["display"]),)
            if value["revision_id"] is not None
        }
        | {
            string(candidate["revision_id"])
            for face in view["face"]
            for raw in array(face["wording"])
            for entry in array(object_value(raw)["candidates"])
            for candidate in (object_value(entry),)
            if candidate["revision_id"] is not None
        }
    )
    annotations: set[str] = set()
    for fragment in fragments:
        part = fragment.value["partition"]
        if fragment.table == "annotation_concept" and part != "bootstrap":
            raise ValueError("Annotation concept must use shared catalog partition")
        if fragment.table == "field_annotation":
            for row in fragment.rows:
                owner = object_value(row["owner"])
                expected = (
                    "history"
                    if owner["kind"] == "face_revision" and owner["id"] not in display
                    else "bootstrap"
                    if (owner["kind"] == "face_revision" and row["field"] == "name")
                    or owner["kind"]
                    in {"keyword", "vocabulary", "product", "product_family"}
                    else "detail"
                )
                if part != expected:
                    raise ValueError("Original annotation column partition mismatch")
                if part == "bootstrap":
                    annotations.add(string(row["annotation_set_id"]))
        if fragment.table == "translation" and part == "bootstrap":
            annotations |= {
                string(row["annotation_set_id"])
                for row in fragment.rows
                if row["annotation_set_id"] is not None
            }
    texts = {string(row["id"]): row for row in view["text_unit"]}
    annotations |= {
        string(
            whole_name(
                texts[string(row["name_unit_id"])], string(row["name_concept_id"])
            )["id"]
        )
        for row in view["face_revision"]
        if row["id"] in display and row["name_concept_id"] is not None
    }
    concepts = {
        string(reference["key" if reference["kind"] == "glossary" else "term_id"])
        for row in view["annotation_set"]
        if row["id"] in annotations
        for occurrence in array(row["occurrences"])
        for reference in (object_value(object_value(occurrence)["reference"]),)
        if reference["kind"] != "vocabulary"
    }
    concepts |= {
        string(row["name_concept_id"])
        for row in view["face_revision"]
        if row["id"] in display and row["name_concept_id"] is not None
    }


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
    errata: dict[str, set[str]] = {
        string(region): set() for region in array(manifest["regions"])
    }
    for row in view["errata"]:
        for raw in array(row["versions"]):
            version = object_value(raw)
            errata[string(row["region"])] |= {
                faces[string(object_value(item)["face_id"])]
                for item in array(version["changes"])
            }
            errata[string(row["region"])] |= {
                prints[string(object_value(item)["printing_id"])]
                for item in array(version["printings"])
            }
    if manifest["qa_card_ids"] != sorted(qa) or manifest["errata_card_ids"] != {
        region: sorted(ids) for region, ids in errata.items()
    }:
        raise ValueError("QA/errata summary mismatch")
    universe: list[JsonValue] = [
        {key: row[key] for key in ("id", "kind", "definition_unit_id", "actions")}
        for row in view["keyword"]
    ]
    if digest(canonical(universe)) != manifest["mechanic_universe_id"]:
        raise ValueError("Mechanic universe hash mismatch")


def _back_references(view: View, manifest: Row) -> None:
    if any(
        row["region"] not in array(manifest["regions"])
        or row["card_id"] == row["from_card_id"]
        for row in view["card_mentioned_by"]
    ):
        raise ValueError("Invalid card-name back reference")


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
    validate_annotations(
        view, tuple(string(lang) for lang in array(manifest.get("languages", [])))
    )
    _back_references(view, manifest)
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
    printing_faces = {
        string(printing["id"]): {
            string(object_value(face)["face_id"]) for face in array(printing["faces"])
        }
        for printing in view["printing"]
    }
    for row in view["printing_image"]:
        if string(row["face_id"]) not in printing_faces[string(row["printing_id"])]:
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
    *,
    eligible: list[Row],
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
        known = {dates[string(item["printing_id"])] for item in eligible} - {None}
        if not known:
            raise ValueError("Latest wording display requires a complete release day")
        latest = max(day for day in known if day is not None)
        newest = [
            item for item in eligible if dates[string(item["printing_id"])] == latest
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


def _preferred_basis(
    candidates: list[Row],
    dates: dict[str, str | None],
    revisions: dict[str, Row],
    units: dict[str, str],
    printings: dict[str, Row],
) -> str | None:
    dated = [
        item for item in candidates if dates[string(item["printing_id"])] is not None
    ]
    if not dated or any(item["revision_id"] is None for item in dated):
        return None
    values = []
    for item in dated:
        revision = revisions[string(item["revision_id"])]
        values.append(
            Wording(
                identity=canonical(
                    {
                        key: revision[key]
                        for key in (
                            "name_unit_id",
                            "class_code",
                            "type_code",
                            "cost",
                            "attack",
                            "defense",
                            "traits",
                            "titles",
                            "special_kinds",
                        )
                    }
                ),
                text=units[string(revision["effect_unit_id"])],
                sections=tuple(
                    units[string(object_value(s)["text_unit_id"])]
                    for s in array(revision["sections"])
                ),
            )
        )
    preferred, _ = preferred_wordings(tuple(values))
    latest = max(dates[string(dated[i]["printing_id"])] or "" for i in preferred)
    return min(
        (
            string(dated[i]["printing_id"])
            for i in preferred
            if dates[string(dated[i]["printing_id"])] == latest
        ),
        key=lambda identifier: (string(printings[identifier]["card_no"]), identifier),
    )


@dataclass(frozen=True)
class _WordingEvidence:
    revisions: dict[str, Row]
    units: dict[str, str]
    printings: dict[str, Row]


def _wording_basis(
    face: Row,
    wording: Row,
    dates: dict[str, str | None],
    candidates: list[Row],
    evidence: _WordingEvidence,
) -> None:
    """Verify semantic selection independently of the producing database."""
    identifier = wording["basis_printing_id"]
    day = wording["basis_released_on"]
    if wording["classification"] != "same_effect":
        if identifier is not None or day is not None:
            raise ValueError("Non-equivalent wording cannot have a shared basis")
        return
    if identifier is None:
        raise ValueError("Equivalent wording requires a basis printing")
    selected = [item for item in candidates if item["printing_id"] == identifier]
    display = object_value(wording["display"])
    if (
        len(selected) != 1
        or selected[0]["revision_id"] is None
        or selected[0]["revision_id"] != display["revision_id"]
        or display["basis"] != "current"
    ):
        raise ValueError("Shared wording basis disagrees with the selected observation")
    if day != dates[string(identifier)]:
        raise ValueError("Wording basis release day disagrees with product evidence")
    if wording["method"] != "manual":
        known = [dates[string(item["printing_id"])] for item in candidates]
        fallback = (
            day is None
            and not any(known)
            and len(candidates) < len(array(wording["candidates"]))
            and any(
                object_value(item)["revision_id"] == display["revision_id"]
                and object_value(item)["basis"] == "latest_observed_no_errata"
                for item in array(face["current"])
            )
        )
        if not fallback and identifier != _preferred_basis(
            candidates, dates, evidence.revisions, evidence.units, evidence.printings
        ):
            raise ValueError(
                "Shared wording basis must use the latest known release after wording preferences"
            )
    if not any(
        object_value(item)["revision_id"] == display["revision_id"]
        and object_value(item)["region"] == wording["region"]
        for item in array(face["current"])
    ):
        raise ValueError("Shared wording basis requires a current")


def _wording(view: View) -> None:
    revisions = {string(row["id"]): row for row in view["face_revision"]}
    printings = {string(row["id"]): row for row in view["printing"]}
    units = {string(row["id"]): string(row["text"]) for row in view["text_unit"]}
    empty = {
        identifier
        for identifier, revision in revisions.items()
        if revision["effect_unit_id"] is not None
        and not units[string(revision["effect_unit_id"])].strip()
    }
    physical = _physical_observations(view, revisions)
    regions_by_face: dict[str, set[str]] = {}
    for (_, face_id), (region, _) in physical.items():
        regions_by_face.setdefault(face_id, set()).add(region)
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
        required = regions_by_face.get(string(face["id"]), set())
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
            candidates = [object_value(item) for item in array(wording["candidates"])]
            eligible = (
                [item for item in candidates if item["revision_id"] not in empty]
                if any(
                    item["revision_id"] is not None and item["revision_id"] not in empty
                    for item in candidates
                )
                else candidates
            )
            _wording_candidates(
                face, wording, physical, dates, revisions, eligible=eligible
            )
            _wording_basis(
                face,
                wording,
                dates,
                eligible,
                _WordingEvidence(revisions, units, printings),
            )
            if wording["common_name"] != common_name(wording, revisions):
                raise ValueError("Pending common name differs from candidates")


def validate_placement(view: View, manifest: Row, fragments: list[Fragment]) -> None:
    """Recompute physical membership independently of producer grouping."""
    selected = profile(string(manifest["format_version"]))
    files = {
        string(object_value(f)["key"]): object_value(f)
        for f in array(manifest["files"])
    }
    entities = (
        {string(r["id"]): r for r in view["face"]},
        {string(r["id"]): r for r in view["face_revision"]},
        {string(r["id"]): string(r["region"]) for r in view["printing"]},
    )
    index = {
        (
            f.file,
            f.table,
            canonical(f.value["owner"]),
            string(f.value["segment"]),
            string(f.value["partition"]),
        ): f
        for f in fragments
    }
    facts = _PlacementFacts.from_view(view)
    sources = _source_locations(fragments, index)
    for fragment in fragments:
        if fragment.table == "__catalog":
            continue
        file = files[fragment.file]
        rows = _placed_rows(fragment, index)
        for row in rows:
            _placement_purpose(fragment, row, file, facts)
            _placement_region(fragment, row, file, entities[2])
            if fragment.table == "field_annotation":
                source = sources.get(
                    (canonical(row["owner"]), string(fragment.value["partition"]))
                )
                if source != (fragment.file, fragment.value["segment"]):
                    raise ValueError("Original annotation segment differs from source")
                continue
            expected_key, segment, expected_fragment = _expected_placement(
                fragment, row, file, files, entities, selected
            )
            if fragment.file != expected_key or file["segment"] != segment:
                raise ValueError("File key does not match fixed band recipe")
            if fragment.value["segment"] != expected_fragment:
                raise ValueError("Fragment segment differs from File scope")


def _expected_placement(
    fragment: Fragment,
    row: Row,
    file: Row,
    files: dict[str, Row],
    entities: tuple[dict[str, Row], dict[str, Row], dict[str, str]],
    selected: Profile,
) -> tuple[str, str, str]:
    purpose = string(file["purpose"])
    owner = object_value(fragment.value["owner"])
    family = string(owner["id"]) if owner["id"] is not None else ""
    if fragment.table in {
        "text_unit",
        "translation",
        "annotation_set",
        "annotation_concept",
    }:
        segment = string(file["segment"])
        physical, _, suffix = (
            segment.rpartition(":band-") if purpose != "catalog" else (segment, "", "0")
        )
        family = physical[7:] if physical.startswith("family-") else ""
        band = int(suffix)
        if (
            owner != {"kind": "global", "id": None}
            or fragment.value["base"] is not None
        ):
            raise ValueError("Invalid local dictionary coordinates")
        return (
            _physical_key(purpose, file["region"], family, band),
            _physical_segment(purpose, family, band),
            segment,
        )
    base = fragment.value["base"]
    source_purpose = (
        string(
            files[string(object_value(object_value(base)["file"])["key"])]["purpose"]
        )
        if base is not None
        else purpose
    )
    band = (
        0
        if source_purpose == "catalog"
        else bucket(
            _entity_key(fragment.table, row, entities[0], entities[1]), selected.buckets
        )
        // selected.width(source_purpose, family)
    )
    segment = _physical_segment(purpose, family, band)
    expected_fragment = (
        source_purpose + ":" + string(object_value(base)["segment"])
        if base is not None
        else segment
    )
    return (
        _physical_key(purpose, file["region"], family, band),
        segment,
        expected_fragment,
    )


def _physical_segment(purpose: str, family: str, band: int) -> str:
    segment = "family-" + family if family else "shared"
    return segment if purpose == "catalog" else segment + ":band-" + str(band)


def _physical_key(purpose: str, region: JsonValue, family: str, band: int) -> str:
    return "/".join(
        (
            purpose,
            string(region) if region is not None else "shared",
            "family-" + quote(family, safe="") if family else "shared",
            str(band),
        )
    )


def _placed_rows(
    fragment: Fragment,
    index: dict[tuple[str, str, bytes, str, str], Fragment],
) -> list[Row]:
    if fragment.value["base"] is None:
        return fragment.rows
    base = object_value(fragment.value["base"])
    return index[
        string(object_value(base["file"])["key"]),
        fragment.table,
        canonical(base["owner"]),
        string(base["segment"]),
        string(base["partition"]),
    ].rows


def _source_locations(
    fragments: list[Fragment],
    index: dict[tuple[str, str, bytes, str, str], Fragment],
) -> dict[tuple[bytes, str], tuple[str, JsonValue]]:
    locations: dict[tuple[bytes, str], tuple[str, JsonValue]] = {}
    for fragment in fragments:
        for row in _placed_rows(fragment, index):
            for owner in row_owners(fragment.table, row):
                locations[canonical(owner), string(fragment.value["partition"])] = (
                    fragment.file,
                    fragment.value["segment"],
                )
    return locations


@dataclass(frozen=True)
class _PlacementFacts:
    defaults: frozenset[JsonValue]
    visible: frozenset[tuple[JsonValue, JsonValue]]

    @classmethod
    def from_view(cls, view: View) -> _PlacementFacts:
        return cls(
            frozenset(
                object_value(r)["default_printing_id"]
                for c in view["card"]
                for r in array(c["regions"])
            ),
            frozenset((p["card_id"], p["region"]) for p in view["printing"]),
        )


def _placement_purpose(
    fragment: Fragment, row: Row, file: Row, facts: _PlacementFacts
) -> None:
    table = fragment.table
    purpose = file["purpose"]
    part = fragment.value["partition"]
    if table == "field_annotation":
        return
    if table in {"text_unit", "translation", "annotation_set"}:
        if (part == "bootstrap") != (purpose == "catalog") or purpose == "images":
            raise ValueError("Dictionary partition differs from File scope")
        return
    if table == "annotation_concept":
        if (
            purpose != "catalog"
            or file["region"] is not None
            or file["segment"] != "shared"
        ):
            raise ValueError("Annotation concept requires shared catalog")
        return
    expected = (
        "catalog"
        if table
        in {"card", "face", "product_family", "vocabulary", "keyword", "search_alias"}
        else "product"
        if table == "product"
        else "images"
        if table in {"image_asset", "image_variant", "printing_image"}
        else "print"
        if table in {"printing_product", "art", "artist", "stamp", "digital_art_link"}
        else "engine"
        if table
        in {
            "card_related",
            "card_mentioned_by",
            "face_rules_name",
            "card_engine_support",
            "rules_name",
            "rules_profile",
            "restriction",
            "mechanic_projection",
            "card_mechanic_coverage",
            "identity_change",
            "digital_link",
            "digital_link_coverage",
            "digital_card",
            "digital_art",
            "voice",
            "card_voice",
            "card_route_alias",
            "route_override",
        }
        else "detail"
    )
    if table == "face_revision":
        expected = "catalog" if part == "bootstrap" else string(part)
    elif table == "printing":
        expected = (
            "print-detail"
            if part == "detail"
            else "catalog"
            if row["id"] in facts.defaults
            else "print"
        )
    elif table in {"card", "face"}:
        card_id = row["id"] if table == "card" else row["card_id"]
        visible = (card_id, file["region"]) in facts.visible
        expected = "catalog" if visible else "region-state"
    if purpose != expected:
        raise ValueError("Table purpose differs from File scope")


def _placement_region(
    fragment: Fragment, row: Row, file: Row, printing_regions: dict[str, str]
) -> None:
    if fragment.table == "card_mentioned_by":
        if file["purpose"] != "engine" or file["region"] is not None:
            raise ValueError("Card-name back references require shared engine")
        return
    if (
        row.get("region") is not None
        and row["region"] != file["region"]
        and file["purpose"] != "images"
    ):
        raise ValueError("Row region differs from File scope")
    if (
        fragment.table == "printing_product"
        and printing_regions[string(row["printing_id"])] != file["region"]
    ):
        raise ValueError("Printing product region differs from parent")


def _entity_key(
    table: str, row: Row, faces: dict[str, Row], revisions: dict[str, Row]
) -> list[JsonValue]:
    if table == "field_annotation":
        return _annotation_entity(row, faces, revisions)
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


def _annotation_entity(
    row: Row, faces: dict[str, Row], revisions: dict[str, Row]
) -> list[JsonValue]:
    owner = object_value(row["owner"])
    if owner["kind"] == "face_revision":
        return [faces[string(revisions[string(owner["id"])]["face_id"])]["card_id"]]
    if owner["kind"] == "printing_face":
        return [owner["id"]]
    return [row["owner"], row["field"], row["ordinal"]]
