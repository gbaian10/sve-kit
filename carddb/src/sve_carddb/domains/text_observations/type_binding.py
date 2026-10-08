"""Classify missing source types only through an exact verified correction application."""

from typing import TYPE_CHECKING

from sve_carddb.domains.source_corrections.plan import verify_application

if TYPE_CHECKING:
    from sve_carddb.domains.text_observations.models import FaceObservation
    from sve_carddb.domains.text_observations.plan import TextPlan
    from sve_carddb.domains.text_observations.vocabulary import Binding, Vocabulary


def type_binding(
    plan: TextPlan, item: FaceObservation, vocabulary: Vocabulary
) -> Binding:
    """Retain the raw revision while taking its code from the actual type correction."""
    return vocabulary.lookup(item.region, "type", type_spelling(plan, item))


def type_spelling(plan: TextPlan, item: FaceObservation) -> str:
    """Resolve missing source types before checking any vocabulary correspondence."""
    if item.content.type_raw != "-":
        return item.content.type_raw
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
    return application.data.corrected_value
