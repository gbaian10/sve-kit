"""Redacted EN integration acceptance, without adopting or publishing any rows."""

from collections import Counter, defaultdict
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build.source_rows import source_values
from sve_carddb.core.provenance import uses_sorted
from sve_carddb.domains.products.plan import plan_official_products
from sve_carddb.domains.registry.records import CorrectionData, PrintingData
from sve_carddb.domains.text_observations.composition import text_preview_uses
from sve_carddb.domains.text_observations.plan import verify_plan

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.domains.products.plan import OfficialProducts
    from sve_carddb.domains.registry.preview.plan import PreviewPlan
    from sve_carddb.domains.text_observations.models import FaceObservation
    from sve_carddb.domains.text_observations.plan import TextPlan

_ACTIONS = {
    "exact": "retain_historic_identity_separate_release_gates",
    "mismatch": "re_review_exact_source_without_rewriting_history",
    "missing_raw": "supply_pinned_source_without_live_fallback",
}


def review_queue(identity: PreviewPlan) -> list[JsonValue]:
    """Find every record affected by missing or changed exact observations."""
    queue: list[JsonValue] = []
    for projection in identity.projections:
        for check in projection.evidence:
            if check.status == "matched":
                continue
            found = identity.evidence.get((check.region, check.card_no))
            queue.append(
                {
                    "record_key": projection.record_key,
                    "region": check.region,
                    "card_no": check.card_no,
                    "status": check.status,
                    "expected_observation_hash": check.observation_hash,
                    "expected_rules_hash": check.rules_hash,
                    "actual_observation_hash": None
                    if found is None
                    else found.observation.observation_hash,
                    "actual_rules_hash": None
                    if found is None
                    else found.observation.rules_hash,
                    "source": None
                    if found is None
                    else found.source.model_dump(mode="json"),
                    "action": _ACTIONS["missing_raw" if found is None else "mismatch"],
                }
            )
    return queue


def acceptance_report(
    texts: TextPlan, official: OfficialProducts, stores: Mapping[str, Path]
) -> dict[str, JsonValue]:
    """Describe planned source uses without attesting materialization or release readiness."""
    if "en" not in texts.identity.regions:
        raise ValueError("EN acceptance requires explicit EN output selection")
    verify_plan(texts)
    if official.preview != texts.identity or official != plan_official_products(
        official.identities, official.pages, texts.identity
    ):
        raise ValueError("EN acceptance product/identity plan mismatch")
    uses = uses_sorted(
        text_preview_uses(official.identities.catalog, texts, stores, official=official)
    )
    rows = _printings(texts, official)
    counts = Counter(str(row["status"]) for row in rows)
    comparable = counts["exact"] + counts["mismatch"]
    return {
        "report_format": 1,
        "regions": list[JsonValue](texts.identity.regions),
        "denominator": len(rows),
        "counts": {status: counts[status] for status in _ACTIONS},
        "rates": {
            "all_registered": {"numerator": counts["exact"], "denominator": len(rows)}
            if rows
            else None,
            "comparable": {"numerator": counts["exact"], "denominator": comparable}
            if comparable
            else None,
            "comparable_denominator": comparable,
        },
        "parse_failures": "abort_without_acceptance_report",
        "authored_revision": official.identities.revision,
        "next_int_id": dict(texts.identity.snapshot.files.index().next_int_id),
        "printings": list[JsonValue](rows),
        "review_queue": review_queue(texts.identity),
        "expected_source_uses": [use.model_dump(mode="json") for use in uses],
        "expected_source_use_counts": dict(Counter(use.usage for use in uses)),
        "source_closure": "planned_inputs_without_independent_closure_replay",
        "publication_gate": False,
        "snapshot_output_authorized": False,
        "release_status": "blocked",
        "remaining_gates": [
            "region_text_review",
            "current_adoption_contract",
            "errata_coverage",
            "public_projection",
            "release_validation",
        ],
    }


def _printings(
    texts: TextPlan, official: OfficialProducts
) -> list[dict[str, JsonValue]]:
    observations: dict[str, list[FaceObservation]] = defaultdict(list)
    for item in texts.observations:
        observations[item.printing_id].append(item)
    groups = {(group.face_id, group.region): group for group in texts.groups}
    pages = {(page.region, page.card_no): page for page in official.pages}
    inclusions: dict[str, list[str]] = defaultdict(list)
    for inclusion in official.inclusions:
        inclusions[inclusion.data.printing_id].append(inclusion.data.product_id)
    diagnostics: dict[str, list[JsonValue]] = defaultdict(list)
    for diagnostic in official.diagnostics:
        if (
            isinstance(diagnostic, dict)
            and diagnostic.get("region") == "en"
            and isinstance(number := diagnostic.get("card_no"), str)
        ):
            diagnostics[number].append(diagnostic)
    applications = {
        application.data.id: application for application in texts.corrections or ()
    }
    corrections: dict[str, list[CorrectionData]] = defaultdict(list)
    for record in texts.identity.snapshot.records.values():
        if isinstance(record.data, CorrectionData):
            corrections[record.data.printing_id].append(record.data)
    rows: list[dict[str, JsonValue]] = []
    for projection in texts.identity.projections:
        record = texts.identity.snapshot.records[projection.record_key]
        data = record.data
        if not isinstance(data, PrintingData) or data.region != "en":
            continue
        evidence = texts.identity.evidence.get(("en", data.card_no))
        status = (
            "missing_raw"
            if evidence is None
            else "exact"
            if evidence.observation == data.observation
            else "mismatch"
        )
        items = sorted(observations[data.id], key=lambda item: item.source_index)
        page = pages.get(("en", data.card_no))
        rows.append(
            {
                "printing_id": data.id,
                "card_id": data.card_id,
                "card_no": data.card_no,
                "status": status,
                "action": _ACTIONS[status],
                "expected": data.observation.model_dump(mode="json"),
                "actual": None
                if evidence is None
                else evidence.observation.model_dump(mode="json"),
                "source": None
                if evidence is None
                else evidence.source.model_dump(mode="json"),
                "source_face_map": [
                    mapping.model_dump(mode="json") for mapping in data.source_face_map
                ],
                "identity_disposition": projection.disposition,
                "identity_reasons": list[JsonValue](projection.reasons),
                "product_source_matches": page is not None
                and evidence is not None
                and source_values(page.source) == source_values(evidence.source),
                "product_blocks": None if page is None else len(page.blocks),
                "product_ids": list[JsonValue](sorted(inclusions[data.id])),
                "product_diagnostics": diagnostics[data.card_no],
                "text_faces": [
                    {
                        "face_id": item.face_id,
                        "source_index": item.source_index,
                        "content_hash": item.content.fingerprint(),
                        "raw_content_hash": item.card.faces[
                            item.source_index
                        ].fingerprint(),
                        "raw_effect_present": item.card.faces[item.source_index].effect
                        is not None,
                        "effect_present": item.content.effect is not None,
                        "effect_presence": item.card.effect_presence[
                            item.source_index
                        ].value()
                        if item.card.effect_presence
                        else None,
                        "section_count": len(item.content.sections),
                        "source": item.card.source.model_dump(mode="json"),
                        "pending_reasons": list[JsonValue](
                            groups[item.face_id, item.region].reasons
                        ),
                    }
                    for item in items
                ],
                "corrections": [
                    applications[correction.id].report()
                    if correction.id in applications
                    else {
                        "correction_id": correction.id,
                        "status": "deferred",
                        "reason": "image_inputs_not_supplied"
                        if texts.corrections is None
                        else "identity_parent_excluded",
                    }
                    for correction in sorted(corrections[data.id], key=lambda c: c.id)
                ],
                "region_text_review": "blocked_not_supplied",
            }
        )
    return rows
