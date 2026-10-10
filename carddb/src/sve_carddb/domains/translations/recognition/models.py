"""Translation-contract span/schema shapes, separate from candidate uncertainty."""

from typing import Literal

from pydantic import JsonValue

from sve_carddb.contracts.four_layer import Span
from sve_carddb.core.models import RecordData, Text, UInt

NumericRule = Literal[
    "suffix_unit_cards",
    "suffix_unit_entities",
    "suffix_unit_points",
    "suffix_unit_times",
    "suffix_unit_turns",
    "suffix_unit_pp",
    "prefix_field_cost",
    "prefix_field_attack",
    "prefix_field_health",
    "prefix_field_pp",
    "prefix_field_level",
]


class Hint(RecordData):
    name: Text
    occurrence: Span
    source_segments: tuple[Span, ...]
    transformation: Text
    semantic_role: Text
    numeric_rule: NumericRule | None
    rule_id: Text | None = None
    type: Literal["uint", "literal", "reference"] | None
    reference_kind: Literal["card", "term", "vocabulary"] | None
    value: UInt | None
    target: dict[str, JsonValue] | None
    issues: tuple[Text, ...]


class LiteralTrace(RecordData):
    occurrence: Span
    source_segments: tuple[Span, ...]


class Candidate(RecordData):
    slots: tuple[Hint, ...]
    literal_trace: tuple[LiteralTrace, ...]
    issues: tuple[Text, ...]
