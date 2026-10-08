"""Keep release, mapping, dates and manual engine support as independent axes."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import array, integer, object_value, string
from sve_carddb.export.project.source import json_list

if TYPE_CHECKING:
    from collections.abc import Mapping

    from pydantic import JsonValue

    from sve_carddb.domains.routes.defaults import DefaultPrinting
    from sve_carddb.export.project.evidence import Decisions
    from sve_carddb.export.project.source import Record, Source


class Dates:
    def __init__(self, source: Source, view: dict[str, list[Record]]) -> None:
        products = {row["id"]: row for row in view["product"]}
        self.by_print: dict[str, list[tuple[str, str | None, str]]] = {}
        for row in source.rows(
            "printing_product",
            "printing_id,product_id,first_available_on,first_available_precision,first_available_raw",
        ):
            product = products.get(row["product_id"])
            if product is None:
                continue
            precision = row["first_available_precision"]
            date = row["first_available_on"]
            if precision is None:
                precision, date = product["date_precision"], product["released_on"]
            self.by_print.setdefault(string(row["printing_id"]), []).append(
                (
                    string(precision),
                    None if date is None else string(date),
                    string(row["product_id"]),
                )
            )

    def earliest(self, printing_id: str) -> str | None:
        """A possibly earlier undated inclusion prevents a first-date claim."""
        rows = self.by_print.get(printing_id, [])
        if not rows or any(
            precision != "day" or date is None for precision, date, _ in rows
        ):
            return None
        return min(string(date) for _, date, _ in rows)

    def debut(self, printings: list[Record]) -> tuple[list[str], str]:
        """Return tied first products only when every possible inclusion is dated."""
        dates = [self.earliest(string(row["id"])) for row in printings]
        if not dates or None in dates:
            return [], "unknown"
        earliest = min(string(date) for date in dates)
        products = {
            product
            for printing in printings
            for _, date, product in self.by_print.get(string(printing["id"]), [])
            if date == earliest
        }
        return sorted(products), "known"


def inclusions(source: Source, view: dict[str, list[Record]], dates: Dates) -> None:
    """Expose date overrides, not duplicated effective dates or fabricated first days."""
    raw = {
        (row["printing_id"], row["product_id"]): row
        for row in source.rows(
            "printing_product",
            "printing_id,product_id,first_available_on,first_available_precision,first_available_raw",
        )
    }
    prints = {row["id"]: row for row in view["printing"]}
    regional: dict[tuple[str, str], list[Record]] = {}
    for printing in view["printing"]:
        regional.setdefault(
            (string(printing["card_id"]), string(printing["region"])), []
        ).append(printing)
    for row in view["printing_product"]:
        original = raw[row["printing_id"], row["product_id"]]
        row.update(
            {
                "available_on": original["first_available_on"],
                "date_precision": original["first_available_precision"],
                "date_raw": original["first_available_raw"],
            }
        )
        printing = prints[row["printing_id"]]
        siblings = regional[string(printing["card_id"]), string(printing["region"])]
        debut, state = dates.debut(siblings)
        row["first_inclusion_state"] = (
            "unknown"
            if state == "unknown"
            else "first"
            if row["product_id"] in debut
            else "reprint"
        )


def _role(
    source: Source, card: Record, region: str, view: dict[str, list[Record]]
) -> str | None:
    for override in source.matching(
        "deck_role_override",
        "card_id,region,role,decision_id",
        card_id=card["id"],
        region=region,
    ):
        if source.review(override["decision_id"]) == "confirmed":
            return string(override["role"])
    currents = {
        object_value(raw)["revision_id"]
        for face in view["face"]
        if face["card_id"] == card["id"]
        for raw in array(face["current"])
        if object_value(raw)["region"] == region
    }
    roles: set[str] = set()
    for revision in view["face_revision"]:
        if revision["id"] not in currents:
            continue
        kinds = set(map(string, array(revision["special_kinds"])))
        if not kinds <= {"token", "ep", "sep", "evolve", "advance"} or revision[
            "type_code"
        ] not in {"leader", "follower", "spell", "amulet"}:
            return None
        roles.add(
            "extra"
            if kinds & {"token", "ep", "sep"}
            else "evolve"
            if kinds & {"evolve", "advance"}
            else "leader"
            if revision["type_code"] == "leader"
            else "main"
        )
    return next(iter(roles)) if len(roles) == 1 else None


def region_views(
    source: Source,
    view: dict[str, list[Record]],
    as_of: str,
    dates: Dates,
    defaults: Mapping[tuple[str, str], DefaultPrinting],
) -> None:
    """No counterpart printing means unknown release, never confirmed non-release."""
    for card in view["card"]:
        own_faces = [face for face in view["face"] if face["card_id"] == card["id"]]
        card["faces"] = [
            face["id"]
            for face in sorted(own_faces, key=lambda row: integer(row["ordinal"]))
        ]
        all_prints = [row for row in view["printing"] if row["card_id"] == card["id"]]
        regions: list[Record] = []
        for region in ("en", "jp"):
            prints = [row for row in all_prints if row["region"] == region]
            counterpart = [row for row in all_prints if row["region"] != region]
            mapping = (
                "confirmed"
                if prints and counterpart and card["identity_state"] == "confirmed"
                else "unmapped"
            )
            reviews = source.matching(
                "region_mapping_review",
                "card_id,target_region,state,as_of,coverage_scope",
                card_id=card["id"],
                target_region="jp" if region == "en" else "en",
            )
            review = (
                max(reviews, key=lambda row: string(row["as_of"])) if reviews else None
            )
            if mapping != "confirmed" and review is not None:
                mapping = (
                    "confirmed_none"
                    if review["state"] == "confirmed_none"
                    else "pending"
                )
            release = (
                "released"
                if any(
                    row["catalog_state"] == "official"
                    or row["review_level"] in {"sampled", "confirmed"}
                    for row in prints
                )
                else "unknown"
            )
            overrides = source.matching(
                "region_availability_override",
                "card_id,region,state,as_of,decision_id",
                card_id=card["id"],
                region=region,
            )
            date = as_of
            if (
                release == "unknown"
                and overrides
                and source.review(overrides[0]["decision_id"]) == "confirmed"
            ):
                release, date = (
                    string(overrides[0]["state"]),
                    string(overrides[0]["as_of"]),
                )
            default = defaults.get((string(card["id"]), region))
            debut, debut_state = dates.debut(prints)
            regions.append(
                {
                    "region": region,
                    "release_state": release,
                    "mapping_state": mapping,
                    "as_of": date,
                    "mapping_as_of": None if review is None else review["as_of"],
                    "mapping_scope": None
                    if review is None
                    else review["coverage_scope"],
                    "default_printing_id": None
                    if default is None
                    else default.printing_id,
                    "default_method": None if default is None else default.method,
                    "deck_role": _role(source, card, region, view),
                    "debut_product_ids": json_list(debut),
                    "debut_state": debut_state,
                }
            )
        card["regions"] = json_list(regions)


def support(
    source: Source, view: dict[str, list[Record]], decisions: Decisions
) -> None:
    """R1 has no DSL; blocks still retain every missing source and regional uncertainty."""
    shared: Record = {
        "status": "missing_dsl",
        "dsl_status": None,
        "dsl_version": None,
        "dsl_id": None,
        "validation_state": "not_applicable",
        "reasons": ["missing_dsl"],
        "reason_detail": None,
        "program_ref": None,
        "ruling_revision_ids": [],
    }
    rows: list[Record] = []
    for card in view["card"]:
        blocks: list[Record] = []
        faces = [face for face in view["face"] if face["card_id"] == card["id"]]
        for raw in array(card["regions"]):
            region = object_value(raw)
            code = string(region["region"])
            reasons = _region_reasons(source, card, faces, region, decisions)
            if reasons:
                blocks.append({"region": code, "reasons": json_list(sorted(reasons))})
        rows.append(
            {
                "card_id": card["id"],
                "shared": shared
                | {"reasons": ["missing_dsl"], "ruling_revision_ids": []},
                "overrides": [],
                "region_blocks": json_list(blocks),
            }
        )
    view["card_engine_support"] = rows


def _region_reasons(
    source: Source,
    card: Record,
    faces: list[Record],
    region: Record,
    decisions: Decisions,
) -> set[str]:
    code = string(region["region"])
    reasons: set[str] = set()
    if card["identity_state"] == "retired":
        reasons.add("retired")
    if card["identity_state"] == "provisional":
        reasons.add("identity_unconfirmed")
    currents = [
        any(object_value(item)["region"] == code for item in array(face["current"]))
        for face in faces
    ]
    if not currents or not all(currents):
        reasons.add("missing_region_source")
    if code == "en" and region["mapping_state"] not in {"confirmed", "confirmed_none"}:
        reasons.add("mapping_unconfirmed")
    if code == "en" and (string(card["id"]), code) not in decisions.aligned_regions:
        reasons.add("region_text_unreviewed")
    if _pending(faces, currents, code):
        reasons.add("wording_pending")
    if _divergent(source, card["id"], code):
        reasons.add("region_divergence")
    reasons.update(_supplemental_reasons(card, code, decisions))
    return reasons


def _supplemental_reasons(card: Record, code: str, decisions: Decisions) -> set[str]:
    reasons: set[str] = set()
    for restriction in decisions.supplemental_restrictions:
        if restriction.reason not in {
            "errata_current_pending",
            "source_printing_missing",
        }:
            raise ValueError("Unknown supplemental restriction reason")
        if (restriction.card_id, restriction.region) == (card["id"], code):
            reasons.add(restriction.reason)
            if restriction.announcement_available is False:
                reasons.add("errata_source_missing")
    return reasons


def _pending(faces: list[Record], currents: list[bool], region: str) -> bool:
    return any(
        any(
            object_value(item)["region"] == region
            for item in array(face.get("wording", []))
        )
        and not present
        for face, present in zip(faces, currents, strict=True)
    )


def _divergent(source: Source, card_id: JsonValue, region: str) -> bool:
    rows = source.matching(
        "region_divergence",
        "card_id,region,field_scope,resolved",
        card_id=card_id,
        region=region,
    )
    return any(
        row["resolved"] is False and row["field_scope"] in {"rules", "all"}
        for row in rows
    )


def effective_support(row: Record, region: str) -> Record:
    """Apply override, then blocks; automatic is derived and never stored twice."""
    chosen = object_value(row["shared"])
    for raw in array(row["overrides"]):
        override = object_value(raw)
        if override["region"] == region:
            chosen = object_value(override["support"])
    reasons = set(map(string, array(chosen["reasons"])))
    blocked = False
    for raw in array(row["region_blocks"]):
        block = object_value(raw)
        if block["region"] == region:
            reasons.update(map(string, array(block["reasons"])))
            blocked = True
    status = (
        "reviewed"
        if blocked and chosen["status"] == "engine_passed"
        else chosen["status"]
    )
    return chosen | {
        "effective_status": status,
        "reasons": json_list(sorted(reasons)),
        "automatic": status == "engine_passed",
    }
