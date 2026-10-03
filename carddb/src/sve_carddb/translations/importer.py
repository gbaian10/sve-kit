"""Adopt exact glossary evidence atomically; never promote candidate confidence."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_db import Json
from sve_carddb.build_db.rows import insert_exact
from sve_carddb.build_inputs import input_record, insert_raw_sources
from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.translations.loader import Snapshot, load_glossary
from sve_carddb.translations.models import (
    AssignmentRecord,
    AuthoredValue,
    ChoiceRecord,
    ConceptRecord,
    DictionaryEntry,
    DigitalName,
    EffectTerm,
    EmphasisRecord,
    Shard,
    SourceValue,
    TermRecord,
    VocabularyRecord,
)
from sve_carddb.translations.name_replay import replay_names
from sve_carddb.translations.sources import Sources, pointer

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database, Value
    from sve_carddb.build_inputs import BuildContext, InputRecord


_SV1_NAME_POINTER_PARTS = 5


@dataclass(frozen=True)
class Inputs:
    root: Path
    repository: Path
    authored_revision: str

    def load(self) -> Snapshot:
        """Re-read complete signed inputs and compare their immutable Git bytes."""
        if not re.fullmatch(r"[0-9a-f]{40}", self.authored_revision):
            raise ValueError("Translation authored revision must be a full Git SHA")
        snapshot = load_glossary(self.root)
        repository = PinnedRepository(self.repository)
        for name, exact in [
            ("translations/index.yaml", snapshot.index),
            *((p, e) for p, e, _ in snapshot.shards),
        ]:
            if repository.read(self.authored_revision, "authored/" + name) != exact:
                raise ValueError(
                    "Translation bytes differ from immutable authored revision"
                )
        return snapshot

    def configuration(self) -> dict[str, JsonValue]:
        """Pin the full adopted entry in F1 rather than trusting caller models."""
        return {
            "translation_authored": {
                "authored_revision": self.authored_revision,
                **self.load().pins(),
            }
        }


def validate_choice(  # ruff: ignore[complex-structure,too-many-branches] -- independent source/concept guards cannot substitute for each other
    record: ChoiceRecord | VocabularyRecord,
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
    official = data.origin in {"official_svwb", "official_sv1"}
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
            not isinstance(evidence, DigitalName)
            and original is not None
            and ja != original
        ):
            raise ValueError("Concept evidence differs from adopted Japanese term")
        if (
            official
            and evidence.target_ref.parser
            != "translation-" + data.origin.removeprefix("official_") + "-v1"
        ):
            raise ValueError("Official origin differs from evidence provider")
        if isinstance(evidence, DictionaryEntry):
            expected = f"/data/{evidence.dictionary_kind}/{evidence.entry_key.replace('~', '~0').replace('/', '~1')}"
            if (
                evidence.jp_ref.locator != expected
                or evidence.target_ref.locator != expected
                or evidence.jp_ref.parser != evidence.target_ref.parser
            ):
                raise ValueError("Official dictionary concept/key mismatch")
        elif isinstance(evidence, DigitalName):
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
    evidence: DigitalName, ref: SourceRef, sources: Sources, db: Database
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
    evidence: DigitalName, ja: str, target: str, lang: str, db: Database
) -> None:
    decisions = {r.values["id"]: r.values for r in db.rows("decision")}
    decision = decisions.get(evidence.decision_id)
    if decision is None or decision["state"] not in {"sampled", "confirmed"}:
        raise ValueError("Digital same-concept decision is unadopted")
    links = [
        r.values
        for r in db.rows("digital_link")
        if r.values["digital_face_id"] == evidence.digital_face_id
        and r.values["decision_id"] == evidence.decision_id
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


def populate_glossary(  # ruff: ignore[complex-structure,too-many-branches] -- histories are validated before any effective choice is inserted
    db: Database, inputs: Inputs, *, build: BuildContext, stores: dict[str, Path]
) -> InputRecord:
    """Compose with an already verified publication identity and frozen digital closure."""
    snapshot = inputs.load()
    configuration = object_value(parse(build.configuration.encode()))
    if (
        configuration.get("translation_authored")
        != inputs.configuration()["translation_authored"]
    ):
        raise ValueError("Build configuration does not pin translation authored bytes")
    sources = Sources(stores, inputs.repository, build)
    originals = {}
    for record, _ in snapshot.records():
        if isinstance(record, TermRecord):
            if record.data.source_ref is None:
                lang, text = "ja", record.data.authored_source_ja
            else:
                lang, text, _ = sources.text(
                    record.data.source_ref, record.data.source_span
                )
            if lang != "ja" or not text:
                raise ValueError("Glossary concept requires exact Japanese source")
            originals[record.data.id] = text
        for evidence in record.evidence:
            sources.text(evidence.source_ref)
    values = {}
    for record, _ in snapshot.records():
        if isinstance(record, (ChoiceRecord, VocabularyRecord)):
            values[record.record_key] = validate_choice(
                record,
                original=originals.get(record.data.term_id)
                if isinstance(record, ChoiceRecord)
                else None,
                sources=sources,
                db=db,
            )
    replay, identity = replay_names(snapshot, originals, inputs, sources)
    sources.uses[:] = replay.uses
    insert_raw_sources(db, (use.source for use in sources.uses))
    for revision, path, checksum in sorted(set(identity.authored_uses)):
        insert_exact(
            db,
            "source_record",
            {
                "id": "authored:name-identity:"
                + digest(canonical([revision, path, checksum]))[7:],
                "kind": "authored",
                "sha256": checksum,
                "authored_path": path,
                "authored_revision": revision,
                "parser_version": "name-identity-v1",
            },
            ("id",),
        )
    _audit(db, snapshot, inputs.authored_revision, sources)
    for record, decision in snapshot.records():
        if isinstance(record, (AssignmentRecord, ConceptRecord)):
            for revision, path, checksum in sorted(set(identity.authored_uses)):
                if revision != identity.record_revisions[record.record_key]:
                    continue
                identifier = (
                    "authored:name-identity:"
                    + digest(canonical([revision, path, checksum]))[7:]
                )
                insert_exact(
                    db,
                    "decision_source",
                    {
                        "decision_id": decision,
                        "source_id": identifier,
                        "role": "name_identity:"
                        + digest(canonical([revision, path]))[7:],
                        "locator": path,
                        "quote": None,
                    },
                    ("decision_id", "source_id", "role"),
                )
    for record, decision in snapshot.effective():
        if isinstance(record, TermRecord):
            db.insert(
                "glossary_term",
                {
                    "id": record.data.id,
                    "category": record.data.category,
                    "source_ja": originals[record.data.id],
                    "concept_key": record.data.concept_key,
                    "decision_id": decision,
                },
            )
        elif isinstance(record, (EmphasisRecord, AssignmentRecord, ConceptRecord)):
            continue
        elif isinstance(record, ChoiceRecord):
            value = values[record.record_key]
            if value is not None:
                db.insert(
                    "glossary_translation",
                    {
                        "term_id": record.data.term_id,
                        "lang": record.data.lang,
                        "text": value[0],
                        "origin": record.data.origin,
                        "source_id": value[1],
                        "decision_id": decision,
                    },
                )
        else:
            raise TypeError(
                "Vocabulary label projection belongs to #53; glossary import is atomic"
            )
    result = input_record(build, sources.uses)
    result.verify(db, build, tuple(sources.uses), complete=False)
    return result


def import_glossary(
    db: Database, inputs: Inputs, *, build: BuildContext, stores: dict[str, Path]
) -> InputRecord:
    """Own an atomic transaction for glossary rows and their entire provenance."""
    with db.transaction():
        return populate_glossary(db, inputs, build=build, stores=stores)


def _refs(value: JsonValue) -> tuple[SourceRef, ...]:
    if isinstance(value, dict):
        if set(value) == set(SourceRef.model_fields):
            return (SourceRef.model_validate_json(canonical(value)),)
        return tuple(ref for item in value.values() for ref in _refs(item))
    if isinstance(value, list):
        return tuple(ref for item in value for ref in _refs(item))
    return ()


def _audit(db: Database, snapshot: Snapshot, revision: str, sources: Sources) -> None:
    for path, exact, _ in (
        ("translations/index.yaml", snapshot.index, b""),
        *snapshot.shards,
    ):
        identifier = "authored:translations:" + digest(
            canonical([revision, path, digest(exact)])
        ).removeprefix("sha256:")
        insert_exact(
            db,
            "source_record",
            {
                "id": identifier,
                "kind": "authored",
                "sha256": digest(exact),
                "authored_path": "authored/" + path,
                "authored_revision": revision,
                "parser_version": "translation-authored-v1",
            },
            ("id",),
        )
        for name, _, content in snapshot.shards:
            if name != path:
                continue
            shard = Shard.model_validate_json(content)
            decision = shard.decisions[0]
            values: dict[str, Value] = {
                k: v
                for k, v in decision.model_dump(
                    mode="json", exclude={"members", "reviewed_precision", "sample_ids"}
                ).items()
                if isinstance(v, str) or v is None
            }
            values["sample_ids"] = Json(list[JsonValue](decision.sample_ids))
            insert_exact(db, "decision", values, ("id",))
            insert_exact(
                db,
                "decision_source",
                {
                    "decision_id": decision.id,
                    "source_id": identifier,
                    "role": "translation_envelope",
                    "locator": path,
                    "quote": None,
                },
                ("decision_id", "source_id", "role"),
            )
            references = {
                (sources.document(ref)[2].id, ref.locator)
                for record in shard.records
                for ref in _refs(record.model_dump(mode="json"))
            }
            for use in sources.uses:
                if (use.source.id, use.locator) not in references:
                    continue
                role = "translation_evidence:" + digest(
                    canonical([use.source.id, use.locator])
                ).removeprefix("sha256:")
                insert_exact(
                    db,
                    "decision_source",
                    {
                        "decision_id": decision.id,
                        "source_id": use.source.id,
                        "role": role,
                        "locator": use.locator,
                        "quote": None,
                    },
                    ("decision_id", "source_id", "role"),
                )
