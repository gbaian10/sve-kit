"""Block only correction-conflicted regional closures, retaining pending wording."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.domains.text_observations.exclusions import reference_exclusions

if TYPE_CHECKING:
    from sve_carddb.build import CompiledSchema, Database
    from sve_carddb.domains.text_observations.plan import TextPlan


def correction_exclusions(
    db: Database, schema: CompiledSchema, plan: TextPlan
) -> dict[str, JsonValue]:
    """Provide exact blocked DB keys for routes, defaults and all downstream references."""
    if plan.corrections is None:
        raise ValueError("Correction output closure requires a pinned correction plan")
    return reference_exclusions(db, schema, plan.identity, plan.publication_identity())
