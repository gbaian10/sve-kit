"""Project public Correction records solely on their affected reference uses."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.domains.source_corrections.importer import verify_corrections
from sve_carddb.domains.text_observations.importer import revision_id
from sve_carddb.domains.text_observations.plan import verify_plan

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.domains.text_observations.plan import TextPlan
    from sve_carddb.domains.text_observations.vocabulary import Vocabulary


def correction_references(
    db: Database, plan: TextPlan, vocabulary: Vocabulary
) -> tuple[dict[str, JsonValue], ...]:
    """Return exact printing-face/source/revision uses for the later snapshot projector."""
    verify_plan(plan)
    verify_corrections(db, plan, vocabulary)
    sources = {row.values["id"]: row.values for row in db.rows("source_record")}
    candidates = {
        (item.printing_id, item.face_id, item.card.source.id): item
        for item in plan.candidates()
    }
    grouped: dict[tuple[str, str, str], list[JsonValue]] = {}
    for application in plan.corrections or ():
        item = application.observation
        url = sources[item.card.source.id]["url"]
        if url is not None and not isinstance(url, str):
            raise TypeError("Correction public source URL must be text or null")
        marker = application.marker(url)
        if marker is not None:
            grouped.setdefault(
                (item.printing_id, item.face_id, item.card.source.id), []
            ).append(marker)
    return tuple(
        {
            "printing_id": printing,
            "face_id": face,
            "source_id": source,
            "revision_id": revision_id(candidates[printing, face, source]),
            "corrections": markers,
        }
        for (printing, face, source), markers in sorted(grouped.items())
    )
