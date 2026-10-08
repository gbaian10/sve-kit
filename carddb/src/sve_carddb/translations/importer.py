"""Adopt current glossary values atomically with exact frozen evidence."""

import re
from dataclasses import dataclass
from functools import cached_property
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.core.json import canonical, object_value
from sve_carddb.translations.current_models import ChoiceRecord as CurrentChoiceRecord
from sve_carddb.translations.current_models import DigitalName as CurrentDigitalName
from sve_carddb.translations.loader import Snapshot, load_glossary
from sve_carddb.translations.models import (
    AuthoredValue,
    DictionaryEntry,
    EffectTerm,
    SourceValue,
)
from sve_carddb.translations.sources import Sources, pointer

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.core.provenance import BuildContext, InputRecord


_SV1_NAME_POINTER_PARTS = 5


@dataclass(frozen=True)
class Inputs:
    root: Path
    repository: Path
    authored_revision: str

    @cached_property
    def snapshot(self) -> Snapshot:
        """Share one working-tree read across all consumers in this command."""
        return load_glossary(self.root)

    def load(self) -> Snapshot:
        """Reuse current inputs within this command."""
        return self.snapshot

    def configuration(self) -> dict[str, JsonValue]:
        """Describe enabled input areas without locking mutable authored bytes."""
        return {"translation_authored": {"authored_revision": self.authored_revision}}


def validate_choice(  # ruff: ignore[complex-structure,too-many-branches,too-many-locals] -- independent source/concept guards cannot substitute for each other
    record: CurrentChoiceRecord,
    *,
    original: str | None,
    sources: Sources,
    db: Database,
) -> tuple[str, str | None] | None:
    """Official claims must prove both exact wording and the adopted concept relation."""
    data = record.data
    if data.value is None:
        return None
    source_id = None
    if isinstance(data.value, AuthoredValue):
        text = data.value.text
    else:
        lang, text, source = sources.text(data.value.source_ref, data.value.span)
        if lang != data.lang:
            raise ValueError("Choice source language mismatch")
        source_id = source.id
    origin = record.origin
    official = origin == "official"
    if official and not data.concept_evidence:
        raise ValueError("Official choice lacks same-concept evidence")
    for evidence in data.concept_evidence:
        jp_span = evidence.jp_span if isinstance(evidence, EffectTerm) else None
        target_span = evidence.target_span if isinstance(evidence, EffectTerm) else None
        jp_lang, ja, _ = sources.text(evidence.jp_ref, jp_span)
        target_lang, target, target_source = sources.text(
            evidence.target_ref, target_span
        )
        if jp_lang != "ja" or target_lang != data.lang or target != text:
            raise ValueError("Concept evidence language/exact target mismatch")
        if (
            not isinstance(evidence, CurrentDigitalName)
            and original is not None
            and ja != original
        ):
            raise ValueError("Concept evidence differs from adopted Japanese term")
        expected_provider = {"translation-sv1-v1", "translation-svwb-v1"}
        if official and evidence.target_ref.parser not in expected_provider:
            raise ValueError("Official origin differs from evidence provider")
        if isinstance(evidence, DictionaryEntry):
            expected = f"/data/{evidence.dictionary_kind}/{evidence.entry_key.replace('~', '~0').replace('/', '~1')}"
            if (
                evidence.jp_ref.locator != expected
                or evidence.target_ref.locator != expected
                or evidence.jp_ref.parser != evidence.target_ref.parser
            ):
                raise ValueError("Official dictionary concept/key mismatch")
        elif isinstance(evidence, CurrentDigitalName):
            _digital_evidence(evidence, ja, target, data.lang, db)
            for ref in (evidence.jp_ref, evidence.target_ref):
                _digital_location(evidence, ref, sources, db)
        elif isinstance(evidence, EffectTerm):
            for ref in (evidence.jp_ref, evidence.target_ref):
                _effect_location(ref)
        source_id = target_source.id
    if (
        official
        and isinstance(data.value, SourceValue)
        and not any(
            data.value.source_ref == evidence.target_ref
            and data.value.span
            == (evidence.target_span if isinstance(evidence, EffectTerm) else None)
            for evidence in data.concept_evidence
        )
    ):
        raise ValueError("Official source value is outside its concept evidence")
    if official and source_id is None:
        raise ValueError("Official choice lacks a frozen source")
    return text, source_id


def _effect_location(ref: SourceRef) -> None:
    patterns = {
        "translation-svwb-v1": r"/data/card_details/[0-9]{8}/(?:common|evo)/skill_text",
        "translation-sv1-v1": r"/data/cards/[0-9]+/(?:org_)?(?:evo_)?skill_disc",
        "translation-jp-v1": r"/faces/[0-9]+/(?:text|sections/[0-9]+)",
    }
    pattern = patterns.get(ref.parser)
    if pattern is None or not re.fullmatch(pattern, ref.locator):
        raise ValueError("Effect-term evidence must locate an effect field")


def _digital_location(
    evidence: CurrentDigitalName,
    ref: SourceRef,
    sources: Sources,
    db: Database,
) -> None:
    faces = {r.values["id"]: r.values for r in db.rows("digital_face")}
    cards = {r.values["id"]: r.values for r in db.rows("digital_card")}
    face = faces[evidence.digital_face_id]
    card = cards[face["digital_card_id"]]
    _, document, _ = sources.document(ref)
    if ref.parser != "translation-" + str(card["game"]) + "-v1":
        raise ValueError("Digital name locator provider mismatch")
    if card["game"] == "svwb":
        expected = f"/data/card_details/{card['official_id']}/common/name"
        if ref.locator != expected:
            raise ValueError("Digital name locator points to another card or field")
    else:
        parts = ref.locator.split("/")
        if (
            len(parts) != _SV1_NAME_POINTER_PARTS
            or parts[1:3] != ["data", "cards"]
            or parts[4] != "card_name"
        ):
            raise ValueError("Digital name locator must reference a card name")
        parent = object_value(pointer(document, "/".join(parts[:-1])))
        if str(parent.get("card_id")) != card["official_id"]:
            raise ValueError("Digital name locator points to another card")


def _digital_evidence(
    evidence: CurrentDigitalName,
    ja: str,
    target: str,
    lang: str,
    db: Database,
) -> None:
    decisions = {r.values["id"]: r.values for r in db.rows("decision")}
    links = [
        r.values
        for r in db.rows("digital_link")
        if r.values["digital_face_id"] == evidence.digital_face_id
        and decisions.get(r.values["decision_id"], {}).get("state")
        in {"sampled", "confirmed"}
        and r.values["relation"] == "same_card"
        and r.values["face_id"] == evidence.sve_owner
    ]
    if not links:
        raise ValueError("Same-character/name-only is not same-concept name evidence")
    units = {r.values["id"]: r.values for r in db.rows("text_unit")}
    text = {
        r.values["lang"]: units[r.values["name_unit_id"]]
        for r in db.rows("digital_text")
        if r.values["digital_face_id"] == evidence.digital_face_id
    }
    if (
        "ja" not in text
        or lang not in text
        or text["ja"]["text"] != ja
        or text[lang]["text"] != target
    ):
        raise ValueError("Digital name evidence does not locate the adopted face names")


def populate_glossary(
    db: Database,
    inputs: Inputs,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    sources: Sources | None = None,
) -> InputRecord:
    """Compose current values with verified publication identity and frozen sources."""
    from sve_carddb.translations.current_importer import populate  # ruff: ignore[import-outside-top-level] -- the projection reuses this module's independent evidence validators

    return populate(
        db, inputs, inputs.load(), build=build, stores=stores, sources=sources
    )


def import_glossary(
    db: Database,
    inputs: Inputs,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    sources: Sources | None = None,
) -> InputRecord:
    """Own an atomic transaction for glossary rows and their entire provenance."""
    with db.transaction():
        return populate_glossary(
            db, inputs, build=build, stores=stores, sources=sources
        )


def _refs(value: JsonValue) -> tuple[SourceRef, ...]:
    if isinstance(value, dict):
        if set(value) == set(SourceRef.model_fields):
            return (SourceRef.model_validate_json(canonical(value)),)
        return tuple(ref for item in value.values() for ref in _refs(item))
    if isinstance(value, list):
        return tuple(ref for item in value for ref in _refs(item))
    return ()
