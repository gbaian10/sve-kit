"""Current matcher IDs shared by switches and positional resolution."""

from typing import Annotated

from pydantic import Field

RuleId = Annotated[str, Field(pattern=r"^[a-z][a-z0-9_]*\Z")]

LEGACY_IDS = (
    "prefix_field_attack",
    "prefix_field_cost",
    "prefix_field_health",
    "suffix_unit_cards",
    "suffix_unit_entities",
    "suffix_unit_pp",
    "suffix_unit_times",
    "suffix_unit_turns",
)
