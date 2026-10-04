"""Preserve final target wording and keep unalignable drafts outside active translations."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical
from sve_carddb.template_translations.current_models import (
    Candidate,
    CandidateRecord,
    Translation,
    TranslationRecord,
)
from sve_carddb.template_translations.migration_alignment import align
from sve_carddb.template_translations.text import escape, verify_flavor

if TYPE_CHECKING:
    from sve_carddb.template_parameters.models import Hint, Schema
    from sve_carddb.template_translations.current_models import DefinitionRecord
    from sve_carddb.template_translations.migration_drafts import (
        EffectDraft,
        FlavorDraft,
    )
    from sve_carddb.template_translations.sources import Reconstructed


@dataclass(frozen=True)
class Pending:
    kind: str
    identifier: str
    text: str
    low_confidence: bool
    reasons: tuple[str, ...]
    entries: tuple[str, ...]


@dataclass(frozen=True)
class Targets:
    records: tuple[TranslationRecord, ...]
    pending: tuple[Pending, ...]
    dispositions: tuple[tuple[str, str, str], ...]


def translation(identifier: str, text: str, *, low: bool) -> TranslationRecord:
    """Converted draft targets retain machine origin independently of earlier reviews."""
    return TranslationRecord(
        record_key=canonical(["template_translation", identifier, "zh-Hant"]).decode(),
        kind="template_translation",
        data=Translation(template_id=identifier, lang="zh-Hant", text=text),
        origin="machine",
        low_confidence=low,
        note="",
    )


def effects(  # ruff: ignore[complex-structure,too-many-branches] -- each old family accounts for every target and unresolved cause
    definitions: tuple[DefinitionRecord, ...],
    members: tuple[Reconstructed, ...],
    drafts: tuple[EffectDraft, ...],
    existing: tuple[TranslationRecord, ...],
    labels: dict[tuple[str, str], str],
) -> Targets:
    """Existing final text wins; safe new targets require semantic slot alignment."""
    actual = {member.entry.id: member for member in members}
    records = {r.record_key: r for r in existing}
    families: dict[str, list[Reconstructed]] = {}
    by_family: dict[str, list[DefinitionRecord]] = {}
    for member in members:
        if member.candidate.legacy_id is not None:
            families.setdefault(member.candidate.legacy_id, []).append(member)
    for record in definitions:
        parent = actual[record.data.inventory_id].candidate.legacy_id
        if parent is not None:
            by_family.setdefault(parent, []).append(record)
    pending = []
    dispositions = []
    for value in drafts:
        draft = value.draft
        family = families.get(draft.template, [])
        reasons = {reason for member in family for reason in member.pending}
        transferred = 0
        for record in by_family.get(draft.template, ()):
            member = actual[record.data.inventory_id]
            if member.normalized != draft.normalized:
                reasons.add("draft_normalized_source_differs")
                continue
            target_key = canonical(
                ["template_translation", record.data.id, "zh-Hant"]
            ).decode()
            if target_key in records:
                transferred += 1
                continue
            try:
                prepared = align(
                    draft,
                    member.normalized,
                    record.data.parameter_schema,
                    _labels(member, record.data.parameter_schema, labels),
                    template_id=record.data.id,
                )
            except ValueError as error:
                reasons.add(str(error))
                continue
            records[target_key] = translation(
                record.data.id, prepared.text, low=value.low_confidence
            )
            transferred += 1
        if not family:
            reasons.add("draft_source_missing")
        elif not by_family.get(draft.template):
            reasons.update(reason for member in family for reason in member.pending)
            if not reasons:
                reasons.add("draft_definition_not_available")
        if reasons:
            pending.append(
                Pending(
                    "effect",
                    draft.template,
                    draft.zh,
                    value.low_confidence,
                    tuple(sorted(reasons)),
                    tuple(sorted(member.entry.id for member in family)),
                )
            )
        dispositions.append(
            (
                "effect",
                draft.template,
                "active_and_pending"
                if transferred and reasons
                else "active"
                if transferred
                else "pending",
            )
        )
    return Targets(
        tuple(records[k] for k in sorted(records)), tuple(pending), tuple(dispositions)
    )


def _labels(
    member: Reconstructed, schema: Schema, labels: dict[tuple[str, str], str]
) -> dict[str, str]:
    result = {}
    for slot in schema.slots:
        values = {
            labels[key]
            for hint in member.hints
            if hint.occurrence in slot.occurrences
            and (key := _reference_key(hint)) is not None
            and key in labels
        }
        if len(values) == 1:
            result[slot.name] = next(iter(values))
    return result


def _reference_key(hint: Hint) -> tuple[str, str] | None:
    target = hint.target
    if target is None:
        return None
    kind = target.get("kind")
    if kind == "term" and isinstance(identifier := target.get("id"), str):
        return "term", identifier
    if kind == "vocabulary":
        vocabulary_kind, code = (
            target.get("vocabulary_kind"),
            target.get("vocabulary_code"),
        )
        if isinstance(vocabulary_kind, str) and isinstance(code, str):
            return vocabulary_kind, code
    return None


def flavors(  # ruff: ignore[complex-structure] -- exact source and target guards stay separate from draft accounting
    definitions: tuple[DefinitionRecord, ...],
    members: tuple[Reconstructed, ...],
    drafts: tuple[FlavorDraft, ...],
    final: tuple[TranslationRecord, ...] = (),
    *,
    quality_flags: dict[str, tuple[str, ...]] | None = None,
) -> Targets:
    """Exact whole-field flavor drafts never pass through the effect normalizer."""
    actual = {member.entry.id: member for member in members}
    physical: dict[str, list[Reconstructed]] = {}
    for member in members:
        if member.entry.role == "flavor":
            physical.setdefault(member.entry.normalized_hash, []).append(member)
    available: dict[str, list[DefinitionRecord]] = {}
    for record in definitions:
        member = actual[record.data.inventory_id]
        if member.entry.role == "flavor":
            available.setdefault(member.entry.normalized_hash, []).append(record)
    final_texts = _final_flavor_targets(final)
    flags = quality_flags or {}
    result = {}
    pending = []
    dispositions = []
    for draft in drafts:
        family = physical.get(draft.source_hash, [])
        reasons = set()
        transferred = 0
        for record in available.get(draft.source_hash, ()):
            member = actual[record.data.inventory_id]
            if member.normalized != draft.source_text:
                raise ValueError("migration_flavor_source_hash_collision")
            selected = final_texts.get(record.data.id)
            text = escape(draft.text) if selected is None else selected.data.text
            try:
                verify_flavor(text, record.data.parameter_schema)
            except ValueError:
                reasons.add("draft_flavor_target_invalid")
                continue
            result[record.record_key] = translation(
                record.data.id,
                text,
                low=draft.low_confidence
                or (selected is not None and selected.low_confidence)
                or bool(flags.get(record.data.id)),
            )
            transferred += 1
        if not family:
            reasons.add("draft_source_missing")
        elif not available.get(draft.source_hash):
            reasons.update(reason for member in family for reason in member.pending)
            if not reasons:
                reasons.add("draft_definition_not_available")
        if reasons:
            pending.append(
                Pending(
                    "flavor",
                    draft.identifier,
                    draft.text,
                    draft.low_confidence,
                    tuple(sorted(reasons)),
                    tuple(sorted(member.entry.id for member in family)),
                )
            )
        dispositions.append(
            (
                "flavor",
                draft.identifier,
                "active_and_pending"
                if transferred and reasons
                else "active"
                if transferred
                else "pending",
            )
        )
    return Targets(
        tuple(result[k] for k in sorted(result)), tuple(pending), tuple(dispositions)
    )


def _final_flavor_targets(
    final: tuple[TranslationRecord, ...],
) -> dict[str, TranslationRecord]:
    targets = {r.data.template_id: r for r in final}
    if len(targets) != len(final) or any(r.data.lang != "zh-Hant" for r in final):
        raise ValueError("migration_final_flavor_targets_must_be_unique_zh_hant")
    return targets


def layouts(definitions: tuple[DefinitionRecord, ...]) -> tuple[TranslationRecord, ...]:
    """Whitespace bindings render their exact raw values, without any source-language text."""
    result = []
    for record in definitions:
        if record.data.source_span.role != "layout":
            continue
        slots = record.data.parameter_schema.slots
        if (
            len(slots) != 1
            or slots[0].type != "literal"
            or len(slots[0].occurrences) != 1
        ):
            raise ValueError(
                "migration_layout_requires_its_single_exact_whitespace_slot"
            )
        target = translation(record.data.id, "{{" + slots[0].name + "}}", low=False)
        result.append(target.model_copy(update={"origin": "project"}))
    return tuple(result)


def candidates(pending: tuple[Pending, ...]) -> tuple[CandidateRecord, ...]:
    """Preserve exact inactive target drafts; neither anonymous markers nor quality enables them."""
    records: dict[str, CandidateRecord] = {}
    for item in pending:
        data = Candidate.model_validate(
            {
                "source_kind": item.kind,
                "candidate_id": item.identifier,
                "lang": "zh-Hant",
                "text": item.text,
                "inventory_ids": tuple(sorted(set(item.entries))),
                "reasons": tuple(sorted(set(item.reasons))),
            }
        )
        key = canonical(
            [
                "template_translation_candidate",
                data.source_kind,
                data.candidate_id,
                data.lang,
            ]
        ).decode()
        value = CandidateRecord(
            record_key=key,
            kind="template_translation_candidate",
            data=data,
            origin="machine",
            low_confidence=item.low_confidence,
            note="",
        )
        previous = records.get(key)
        if previous is not None and previous != value:
            raise ValueError(
                "Migration candidate keys contain conflicting final drafts"
            )
        records[key] = value
    return tuple(records[key] for key in sorted(records))
