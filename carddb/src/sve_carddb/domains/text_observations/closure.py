"""Close regional quarantine over identity references without rewriting authored data."""

from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.domains.registry.records import (
    AllocationData,
    ArtData,
    CardData,
    FaceData,
    MappingReviewData,
    PrintingData,
    RelatedData,
)

if TYPE_CHECKING:
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.registry.preview import PreviewPlan


def close_preview(
    preview: PreviewPlan, blocked: frozenset[tuple[str, Region]]
) -> PreviewPlan:
    """Exclude whole regional cards, their physical/ID uses, and dependent references."""
    printings = {
        r.data.id: r.data
        for r in preview.included("printing")
        if isinstance(r.data, PrintingData)
    }
    selected = {
        key: data
        for key, data in printings.items()
        if (data.card_id, data.region) not in blocked
    }
    regions: dict[str, set[Region]] = {}
    for printing in selected.values():
        regions.setdefault(printing.card_id, set()).add(printing.region)
    projections = []
    for projection in preview.projections:
        if projection.disposition != "included":
            projections.append(projection)
            continue
        data = preview.snapshot.records[projection.record_key].data
        allowed = projection.regions
        if isinstance(data, PrintingData):
            allowed = (data.region,) if data.id in selected else ()
        elif isinstance(data, AllocationData):
            allowed = (
                (selected[data.printing_id].region,)
                if data.printing_id in selected
                else ()
            )
        elif isinstance(data, (CardData, FaceData, MappingReviewData)):
            identifier = data.id if isinstance(data, CardData) else data.card_id
            allowed = tuple(sorted(regions.get(identifier, ())))
        elif isinstance(data, ArtData):
            allowed = tuple(
                sorted(
                    {
                        selected[use.printing_id].region
                        for use in data.uses
                        if use.printing_id in selected
                    }
                )
            )
        elif isinstance(data, RelatedData):
            allowed = tuple(
                region
                for region in projection.regions
                if all(
                    region in regions.get(identifier, ())
                    for identifier in (data.from_card_id, data.to_card_id)
                )
            )
        projections.append(
            replace(
                projection,
                regions=allowed,
                disposition="included" if allowed else "excluded",
                reasons=() if allowed else ("text_quarantine_dependency",),
            )
        )
    return replace(preview, projections=tuple(projections))
