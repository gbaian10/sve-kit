"""Independent 2.0 cross-file media checks, without a producer or build DB."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import array, integer, object_value, string

if TYPE_CHECKING:
    from sve_carddb.snapshot.reader import Fragment, Row, View

SIZES = {"art_m", "art_s", "card_l", "card_m", "card_s"}


def validate_media(
    view: View, fragments: list[Fragment], files: dict[str, Row], config_ref: Row
) -> None:
    """Require the public media descriptor and its exact bootstrap closure."""
    print_locations: dict[str, str] = {}
    face_locations: dict[str, str] = {}
    for fragment in fragments:
        if fragment.value["partition"] == "bootstrap" and fragment.table in {
            "printing",
            "face",
        }:
            locations = (
                print_locations if fragment.table == "printing" else face_locations
            )
            for row in fragment.rows:
                locations[string(row["id"])] = fragment.file
    needed: dict[str, set[str]] = {
        key: {string(config_ref["key"])}
        for key, f in files.items()
        if f["role"] == "images"
    }
    for fragment in fragments:
        if fragment.table != "printing_image":
            continue
        for row in fragment.rows:
            needed[fragment.file] |= {
                print_locations[string(row["printing_id"])],
                face_locations[string(row["face_id"])],
            }
    for key, keys in needed.items():
        expected = [{"key": k, "sha256": files[k]["sha256"]} for k in sorted(keys)]
        if files[key]["dependencies"] != expected:
            raise ValueError("Media dependencies must equal exact bootstrap closure")
    _rows(view)
    _identities(view)


def _rows(view: View) -> None:
    variants = {
        (string(r["image_id"]), string(r["size_key"])): r for r in view["image_variant"]
    }
    for row in view["printing_image"]:
        active = (
            row["publication_state"] == "approved"
            and row["availability"] == "available"
        )
        display = [object_value(v) for v in array(row["variants"])]
        keys = [string(v["size_key"]) for v in display]
        if active:
            if row["card_version"] is None or row["art_version"] is None:
                raise ValueError("Available media requires both image versions")
            if keys != sorted(SIZES):
                raise ValueError(
                    "Available media requires sorted five display variants"
                )
            for size in display:
                variant = variants.get(
                    (string(row["image_id"]), string(size["size_key"]))
                )
                if variant is None or any(
                    size[f] != variant[f] for f in ("width", "height")
                ):
                    raise ValueError("Media dimensions differ from image variant")
        elif (
            display or row["card_version"] is not None or row["art_version"] is not None
        ):
            raise ValueError("Unavailable media cannot provide image URLs")


def _identities(view: View) -> None:
    int_ids = [integer(p["int_id"]) for p in view["printing"]]
    if len(set(int_ids)) != len(int_ids):
        raise ValueError("Printing integer identities must be unique")
    positions = [(string(r["card_id"]), integer(r["ordinal"])) for r in view["face"]]
    if len(set(positions)) != len(positions):
        raise ValueError("Face ordinals must be unique within their card")
    for link in view["digital_link"]:
        if link["relation"] == "same_name" and (
            any(
                link[f] is not None
                for f in ("face_id", "digital_phase", "effect_similarity")
            )
            or link["review_level"] != "unreviewed"
        ):
            raise ValueError("Same-name browsing must be unreviewed and card-level")


def validate_digital(view: View, config: Row) -> None:
    """Name-policy browsing is distinct from human same-card review."""
    endpoints = [object_value(v) for v in array(config["digital_endpoints"])]
    games = [string(e["game"]) for e in endpoints]
    if games != ["sv1", "svwb"]:
        raise ValueError("Media config requires both sorted game endpoints")
    if [e["refresh_policy"] for e in endpoints] != ["frozen", "on_sve_release"]:
        raise ValueError("Digital endpoint refresh policy differs from game")
    if any(string(c["game"]) not in games for c in view["digital_card"]):
        raise ValueError("Digital card requires its game endpoint")
    rules = [r for r in view["digital_link"] if r["relation"] == "same_name"]
    pairs = [(string(r["card_id"]), string(r["digital_card_id"])) for r in rules]
    human = {
        (string(r["card_id"]), string(r["digital_card_id"]))
        for r in view["digital_link"]
        if r["relation"] != "same_name"
    }
    if len(set(pairs)) != len(pairs) or set(pairs) & human:
        raise ValueError("Duplicate or human-shadowed same-name card pair")
