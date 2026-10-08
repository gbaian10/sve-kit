"""Validate current glossary keys and references without review history."""

from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.translations.current_models import (
    AssignmentRecord,
    ChoiceRecord,
    ConceptRecord,
    EmphasisRecord,
    Record,
    Shard,
    TermRecord,
)

if TYPE_CHECKING:
    from sve_carddb.translations.loader import Snapshot


def records(snapshot: Snapshot) -> tuple[Record, ...]:
    """Read detached current values from every verified glossary shard."""
    return tuple(
        sorted(
            (
                record
                for _, _, content in snapshot.shards
                for record in Shard.model_validate_json(content).records
            ),
            key=lambda record: record.record_key,
        )
    )


def semantic_hash(record: Record) -> str:
    """Notes are not semantic dependencies, and cannot change a rendered ID."""
    return digest(canonical(record.model_dump(mode="json", exclude={"note"})))


def validate(snapshot: Snapshot) -> None:  # ruff: ignore[complex-structure,too-many-branches] -- distinct record kinds retain their individual semantic references
    """Check current selection keys and references, never private evidence or ancestry."""
    current = records(snapshot)
    keys = [r.record_key for r in current]
    if len(set(keys)) != len(keys):
        raise ValueError("Duplicate current translation selection key")
    terms = {r.data.id: r for r in current if isinstance(r, TermRecord)}
    concepts = [r.data.concept_key for r in terms.values()]
    if len(set(concepts)) != len(concepts):
        raise ValueError("Duplicate current glossary concept key")
    for record in current:
        if isinstance(record, TermRecord):
            if record.data.id != "term:" + record.data.concept_key:
                raise ValueError("Permanent term ID differs from concept key")
        elif isinstance(record, ChoiceRecord):
            if record.data.term_id not in terms:
                raise ValueError("Glossary choice references an absent concept")
            if (
                record.origin == "official"
                and record.data.value is not None
                and not record.data.concept_evidence
            ):
                raise ValueError("Official choice lacks same-concept evidence")
        elif isinstance(record, EmphasisRecord):
            term = terms.get(record.data.term_id)
            if term is None or term.data.category != "rule_term":
                raise ValueError("Emphasis requires an existing rule-term concept")
        elif isinstance(record, ConceptRecord):
            term = terms.get(record.data.term_id or "")
            if record.data.term_id is not None and (
                term is None or term.data.category != "card_name"
            ):
                raise ValueError("Name concept requires an existing card-name term")
        elif isinstance(record, AssignmentRecord):
            term = terms.get("term:" + (record.data.concept_key or ""))
            if record.data.concept_key is not None and (
                term is None or term.data.category != "card_name"
            ):
                raise ValueError(
                    "Name assignment requires an existing card-name concept"
                )
            if record.data.variant != "default" and (
                not record.data.reason.strip() or term is None
            ):
                raise ValueError(
                    "Nondefault name variant requires a concept and reason"
                )
