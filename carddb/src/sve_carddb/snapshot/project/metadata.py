"""Public configuration and sparse summaries preserve unknown coverage."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.core.json import array, canonical, digest, object_value, parse, string
from sve_carddb.snapshot.contract import definition, validate
from sve_carddb.snapshot.project.source import Record, Source, json_list, pick
from sve_carddb.snapshot.semantics import validate_config

if TYPE_CHECKING:
    from sve_carddb.snapshot.project.evidence import Decisions


@dataclass(frozen=True)
class Settings:
    catalog_feedback_url: str
    grammar_version: str
    normalizer_version: str


def configuration(source: Source, settings: Settings) -> Record:
    """Use one authoritative DB/config input; do not expose internal health timestamps."""
    sizes = object_value(
        object_value(definition("Config")["properties"])["image_sizes"]
    )["const"]
    for size in source.rows("image_size", "key,purpose,max_width,max_height"):
        if size not in array(sizes):
            raise ValueError("Build image size differs from fixed public config")
    config: Record = {
        "format_version": "2.0.0",
        "languages": json_list(
            source.rows("language", "code,fallback_order,display_name")
        ),
        "digital_endpoints": json_list(
            source.rows(
                "digital_endpoint",
                "game,card_url_template,language_map,status,refresh_policy",
            )
        ),
        "shop_links": json_list(
            source.rows(
                "shop_link_template",
                "id,url_template,parameters,feature_key,enabled_dev,enabled_prod",
            )
        ),
        "image_sizes": parse(canonical(sizes)),
        "search": {
            "grammar_version": settings.grammar_version,
            "normalizer_version": settings.normalizer_version,
        },
        "catalog_feedback_url": settings.catalog_feedback_url,
        "third_party_image_policy": "mirror_reviewed",
        "deck_eligibility_policy": "regional_decklog",
    }
    if any(
        row["normalizer_version"] != settings.normalizer_version
        for row in source.rows("search_alias", "normalizer_version")
    ):
        raise ValueError("Search alias/config normalizer mismatch")
    validate("Config", config)
    validate_config(config)
    return config


def summaries(
    source: Source,
    view: dict[str, list[Record]],
    regions: tuple[str, ...],
    decisions: Decisions,
) -> Record:
    """Sparse QA/errata IDs do not assert absence outside explicit source windows."""
    current = {row["current_version_id"] for row in view["qa"]}
    qa_ids = sorted(
        {
            string(identifier)
            for row in view["qa_version"]
            if row["id"] in current
            for identifier in array(row["cards"])
        }
    )
    faces = {row["id"]: row["card_id"] for row in view["face"]}
    prints = {row["id"]: row["card_id"] for row in view["printing"]}
    errata_ids: set[str] = set()
    for errata in view["errata"]:
        for raw in array(errata["versions"]):
            version = object_value(raw)
            errata_ids.update(
                string(faces[object_value(item)["face_id"]])
                for item in array(version["changes"])
            )
            errata_ids.update(
                string(prints[object_value(item)["printing_id"]])
                for item in array(version["printings"])
            )
    windows: list[Record] = []
    for row in source.rows(
        "source_coverage",
        "kind,region,scope_key,from_date,until_date,as_of,state,source_id",
    ):
        if row["region"] not in regions:
            continue
        scope = string(row["scope_key"])
        products = {string(product["id"]) for product in view["product"]}
        if scope != "region:*" and not (
            scope.startswith("product:") and scope.removeprefix("product:") in products
        ):
            raise ValueError("Source coverage scope is not public")
        windows.append(
            pick(row, "kind,region,scope_key,from_date,until_date,as_of,state")
            | {"source_url": source.url(row["source_id"])}
        )
    universe = [
        pick(row, "id,kind,definition_unit_id,actions") for row in view["keyword"]
    ]
    mechanics = _mechanics(view, regions, decisions)
    restrictions = [
        pick(row, "profile_id,from_date,until_date,state")
        | {"source_url": source.url(row["source_id"])}
        for row in source.rows(
            "restriction_coverage", "profile_id,from_date,until_date,state,source_id"
        )
        if row["profile_id"] in {profile["id"] for profile in view["rules_profile"]}
    ]
    result: Record = {
        "qa_card_ids": json_list(qa_ids),
        "errata_card_ids": json_list(sorted(errata_ids)),
        "source_windows": json_list(windows),
        "restriction_coverage": json_list(restrictions),
        "mechanic_universe_id": digest(canonical(json_list(universe))),
        "coverage": {
            "reviews": [],
            "translations": [],
            "mechanics": json_list(mechanics),
        },
        "engine_support_target": {
            "engine_version": None,
            "engine_build_hash": None,
            "validation_policy_id": None,
        },
    }

    validate("Coverage", result["coverage"])
    validate("Target", result["engine_support_target"])
    for name, kind in (
        ("source_windows", "SourceWindow"),
        ("restriction_coverage", "RestrictionCoverage"),
    ):
        for value in array(result[name]):
            validate(kind, value)
    return result


def _decode_coverage(row: Record, universe: set[str]) -> tuple[set[str], set[str]]:
    ids = set(map(string, array(row["complete_keyword_ids"])))
    full = (
        universe
        if row["complete_all"] is True
        else universe - ids
        if row["complete_mode"] == "exclude"
        else ids
    )
    partial_ids = set(map(string, array(row["partial_keyword_ids"])))
    remaining = universe - full
    partial = (
        remaining - partial_ids if row["partial_mode"] == "exclude" else partial_ids
    )
    if full & partial or not ids <= universe or not partial_ids <= remaining:
        raise ValueError("Mechanic coverage set mismatch")
    if row["complete_all"] is True:
        if (
            row["complete_mode"] != "include"
            or ids
            or row["partial_mode"] != "include"
            or partial_ids
        ):
            raise ValueError("Complete-all coverage encoding mismatch")
    else:
        for mode, stored, positive, base in (
            (row["complete_mode"], ids, full, universe),
            (row["partial_mode"], partial_ids, partial, remaining),
        ):
            expected = "include" if len(positive) <= len(base - positive) else "exclude"
            if mode != expected or len(stored) > len(base) // 2:
                raise ValueError("Mechanic coverage is not shortest canonical encoding")
    return full, partial


def _eligible_scope(
    card_id: str, region: str, view: dict[str, list[Record]], decisions: Decisions
) -> str | None:
    if region == "jp":
        return "shared"
    support = next(
        row for row in view["card_engine_support"] if row["card_id"] == card_id
    )
    blocked = any(
        object_value(raw)["region"] == region for raw in array(support["region_blocks"])
    )
    if blocked:
        return None
    if any(
        row["card_id"] == card_id and row["scope"] == "en_override"
        for row in view["card_mechanic_coverage"]
    ):
        return "en_override"
    return "shared" if (card_id, region) in decisions.aligned_regions else None


def _mechanics(
    view: dict[str, list[Record]], regions: tuple[str, ...], decisions: Decisions
) -> list[Record]:
    universe = {string(row["id"]) for row in view["keyword"]}
    decoded = {
        (string(row["card_id"]), string(row["scope"])): (
            *_decode_coverage(row, universe),
            row["complete_all"] is True,
        )
        for row in view["card_mechanic_coverage"]
    }
    result: list[Record] = []
    for region in regions:
        cards = {
            string(row["card_id"])
            for row in view["printing"]
            if row["region"] == region
        } & {
            string(row["id"])
            for row in view["card"]
            if row["identity_state"] != "retired"
        }
        complete: dict[str, set[str]] = {}
        annotated: set[str] = set()
        fully_annotated = 0
        for card_id in cards:
            scope = _eligible_scope(card_id, region, view, decisions)
            if scope is None:
                continue
            coverage = decoded.get((card_id, scope))
            if coverage is not None:
                full, partial, complete_all = coverage
                fully_annotated += int(
                    full == universe and (bool(universe) or complete_all)
                )
                complete[card_id] = full
                if (
                    full
                    or partial
                    or any(
                        row["card_id"] == card_id
                        and row["scope"] == scope
                        and row["complete_all"] is True
                        for row in view["card_mechanic_coverage"]
                    )
                ):
                    annotated.add(card_id)
            if any(
                row["card_id"] == card_id and row["scope"] == scope
                for row in view["mechanic_projection"]
            ):
                annotated.add(card_id)
        result.append(
            {
                "region": region,
                "scope": "shared" if region == "jp" else "en_override",
                "total_cards": len(cards),
                "any_annotated_cards": len(annotated),
                "fully_annotated_cards": fully_annotated,
                "unknown_cards": len(cards - annotated),
                "eligibility": "all_non_retired_cards_in_region",
                "by_keyword": [
                    {
                        "keyword_id": keyword,
                        "complete_cards": sum(
                            keyword in full for full in complete.values()
                        ),
                    }
                    for keyword in sorted(universe)
                ],
            }
        )
    return result
