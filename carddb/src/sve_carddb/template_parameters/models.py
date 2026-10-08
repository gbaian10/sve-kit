"""Translation-contract span/schema shapes, separate from candidate uncertainty."""

from typing import Literal

from pydantic import JsonValue

from sve_carddb.contracts.template_parameters import Range, Schema, SourceSpan
from sve_carddb.core.models import Hash, RecordData, Text, UInt

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
    occurrence: Range
    source_segments: tuple[Range, ...]
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
    occurrence: Range
    source_segments: tuple[Range, ...]


class Candidate(RecordData):
    inventory_id: Text
    ordinal: UInt
    line_ordinal: UInt
    source_span: SourceSpan
    normalizer_id: Text
    normalized_hash: Hash
    parameter_normalizer_id: Text
    template_normalized_hash: Hash
    parameter_schema: Schema | None
    slots: tuple[Hint, ...]
    literal_trace: tuple[LiteralTrace, ...]
    issues: tuple[Text, ...]
    signature_hash: Hash
    payload_hash: Hash | None
    adoption_status: Literal["candidate_only"] = "candidate_only"
