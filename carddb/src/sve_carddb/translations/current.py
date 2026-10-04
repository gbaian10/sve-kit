"""Read current values without creating reviews or replaying historical decisions."""

from typing import TYPE_CHECKING

from pydantic import TypeAdapter

from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.translations.current_models import (
    AssignmentRecord,
    ChoiceRecord,
    ConceptRecord,
    EmphasisRecord,
    Record,
    Shard,
    TermRecord,
    key,
)

if TYPE_CHECKING:
    from sve_carddb.translations.loader import Snapshot
    from sve_carddb.translations.models import Record as LegacyRecord

CURRENT_FORMAT = 2
ADAPTER = TypeAdapter[Record](Record)


def convert(record: LegacyRecord, *, low_confidence: bool = False) -> Record:
    """Extract an effective legacy value; approval metadata is deliberately omitted."""
    data = record.data.model_dump(mode="json")
    origin = data.pop("origin", "project")
    if origin in {"official_sv1", "official_svwb"}:
        origin = "official"
    elif origin == "community":
        origin = "project"
    for field in ("adoption_review", "adoption_no", "predecessor", "identity_basis"):
        data.pop(field, None)
    for evidence in data.get("concept_evidence", []):
        if isinstance(evidence, dict):
            evidence.pop("decision_id", None)
            if "concept_note" in evidence:
                evidence["concept_note"] = "同概念依精確來源定位核對。"
    claim = data.get("source_claim")
    if isinstance(claim, dict):
        claim["note"] = "保留所列翻譯來源主張，不據此認定為官方譯名。"
    raw = {
        "record_key": record.record_key,
        "kind": record.kind,
        "data": data,
        "origin": origin,
        "low_confidence": low_confidence,
        "note": "",
    }
    # Validation precedes key derivation; the legacy revision is not a selection key.
    typed = ADAPTER.validate_json(canonical(raw))
    return ADAPTER.validate_json(canonical({**raw, "record_key": key(typed)}))


def records(snapshot: Snapshot) -> tuple[Record, ...]:
    """Normalize legacy terminal values and current values into one detached view."""
    from sve_carddb.translations.loader import Snapshot as LegacySnapshot  # ruff: ignore[import-outside-top-level] -- the legacy snapshot owns format-one chain validation

    legacy = LegacySnapshot(
        snapshot.index,
        tuple(
            f
            for f in snapshot.shards
            if object_value(parse(f[2]))["translation_authored_format"] == 1
        ),
    )
    current = [convert(r) for r, _ in legacy.effective()]
    for _, _, content in snapshot.shards:
        if (
            object_value(parse(content))["translation_authored_format"]
            == CURRENT_FORMAT
        ):
            current.extend(Shard.model_validate_json(content).records)
    return tuple(sorted(current, key=lambda r: r.record_key))


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
        if record.record_key != key(record):
            raise ValueError("Current translation selection key mismatch")
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
