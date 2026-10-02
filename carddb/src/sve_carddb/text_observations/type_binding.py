"""Classify missing source types only through an exact verified correction application."""

from typing import TYPE_CHECKING

from sve_carddb.source_corrections.plan import verify_application

if TYPE_CHECKING:
    from sve_carddb.text_observations.models import FaceObservation
    from sve_carddb.text_observations.plan import TextPlan
    from sve_carddb.text_observations.vocabulary import Binding, Vocabulary


def type_binding(
    plan: TextPlan, item: FaceObservation, vocabulary: Vocabulary
) -> tuple[Binding, str | None]:
    """Retain the raw revision while attributing its code to the actual type correction."""
    if item.content.type_raw != "-":
        return vocabulary.lookup(item.region, "type", item.content.type_raw), None
    applications = [
        application
        for application in plan.corrections or ()
        if application.observation == item
        and application.data.field == "card_type"
        and application.status == "applied"
    ]
    if len(applications) != 1:
        raise ValueError(
            "Missing raw type requires exactly one applicable source correction"
        )
    application = applications[0]
    verify_application(plan.identity.snapshot, application)
    binding = vocabulary.lookup(item.region, "type", application.data.corrected_value)
    assert application.record.decision_id is not None
    return binding, application.record.decision_id
