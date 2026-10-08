"""Validate adopted reskins per region against exact endpoint sources and current."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.domains.card_extras.reskin_rules import actual_rules, expected_rules
from sve_carddb.domains.registry.records import RelatedData
from sve_carddb.domains.text_observations.importer import revision_id
from sve_carddb.domains.text_observations.plan import verify_plan

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.registry.preview.plan import Projection
    from sve_carddb.domains.text_observations import Vocabulary
    from sve_carddb.domains.text_observations.plan import TextPlan


@dataclass(frozen=True)
class _Graph:
    printings: Mapping[Value, Mapping[str, Value]]
    current: Mapping[tuple[Value, Value], Value]
    faces: Mapping[Value, Value]
    rules: Mapping[str, bytes]
    vocabulary: Vocabulary


def applicable_reskin_regions(
    db: Database, texts: TextPlan, *, vocabulary: Vocabulary
) -> dict[str, tuple[Region, ...]]:
    """Return only valid display regions; never propagate DSL or deck identity."""
    verify_plan(texts)
    rows = {row.values["id"]: row.values for row in db.rows("card_related")}
    graph = _Graph(
        {row.values["id"]: row.values for row in db.rows("printing")},
        {
            (row.values["face_id"], row.values["region"]): row.values["revision_id"]
            for row in db.rows("face_current")
        },
        {row.values["id"]: row.values["card_id"] for row in db.rows("face")},
        actual_rules(db),
        vocabulary,
    )
    result: dict[str, tuple[Region, ...]] = {}
    for projection in texts.identity.projections:
        record = texts.identity.snapshot.records[projection.record_key]
        if (
            not isinstance(record.data, RelatedData)
            or projection.disposition != "included"
        ):
            continue
        data = record.data
        row = rows.get(data.id)
        if (
            row is None
            or row["from_card_id"] != data.from_card_id
            or row["to_card_id"] != data.to_card_id
        ):
            continue
        regions = tuple(
            region
            for region in projection.regions
            if _region_valid(texts, data, region, projection, graph)
        )
        if regions:
            result[data.id] = regions
    return result


def _region_valid(
    texts: TextPlan,
    data: RelatedData,
    region: Region,
    projection: Projection,
    graph: _Graph,
) -> bool:
    endpoints = {data.from_card_id, data.to_card_id}
    expected = {e.card_no for e in data.evidence if e.region == region}
    observations = [
        item
        for item in texts.observations
        if item.region == region and item.card_id in endpoints
    ]
    if {item.card_id for item in observations} != endpoints or {
        item.card_no for item in observations
    } != expected:
        return False
    adopted_printings = {
        identifier
        for identifier, printing in graph.printings.items()
        if printing["region"] == region and printing["card_id"] in endpoints
    }
    faces = {face for face, card in graph.faces.items() if card in endpoints}
    if {item.printing_id for item in observations} != adopted_printings or {
        item.face_id for item in observations
    } != faces:
        return False
    for item in observations:
        printing = graph.printings.get(item.printing_id)
        if printing is None or (
            printing["region"],
            printing["card_no"],
            printing["card_id"],
            printing["source_id"],
        ) != (region, item.card_no, item.card_id, item.card.source.id):
            return False
        proof = texts.identity.evidence.get((region, item.card_no))
        if (
            proof is None
            or proof.source.id != item.card.source.id
            or proof.observation != item.card.observation
            or not any(
                check.status == "matched" and check.source_id == item.card.source.id
                for check in projection.evidence
            )
        ):
            return False
        if graph.current.get((item.face_id, region)) != revision_id(
            item
        ) or graph.rules.get(revision_id(item)) != expected_rules(
            item, graph.vocabulary
        ):
            return False
    return True
