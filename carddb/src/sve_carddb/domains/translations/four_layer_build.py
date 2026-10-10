"""Project authored frames only after replaying this build's exact Japanese owner fields."""

from collections import Counter
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build.rows import insert_exact
from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.contracts.four_layer import (
    FaceRevisionOwner,
    OwnerField,
    PrintingFaceOwner,
)
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.translations.four_layer_authored import (
    FormRecord,
    FrameRecord,
    TargetRecord,
    TargetVariantRecord,
    shard,
)
from sve_carddb.domains.translations.four_layer_fields import render_field
from sve_carddb.domains.translations.four_layer_labels import labels
from sve_carddb.domains.translations.four_layer_matching import Frames
from sve_carddb.domains.translations.four_layer_pipeline import (
    compile_card_field,
    prepare_classifier,
)
from sve_carddb.domains.translations.four_layer_render import (
    Form,
    Renderer,
    SelectedTarget,
)
from sve_carddb.domains.translations.four_layer_selection import Controls
from sve_carddb.domains.translations.four_layer_sources import descriptor, empty_field
from sve_carddb.domains.translations.four_layer_storage import (
    write_form,
    write_frame,
    write_target,
)

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build import Database
    from sve_carddb.domains.text_observations.vocabulary import Vocabulary
    from sve_carddb.domains.translations.four_layer_authored import Inputs
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_pipeline import CompiledField
    from sve_carddb.domains.translations.inputs import Snapshot
    from sve_carddb.domains.translations.recognition.rules import Rules
    from sve_carddb.domains.translations.sources import Sources


@dataclass(frozen=True)
class Report:
    fields: int
    translated: int
    low_confidence: int
    reasons: Counter[str]
    pending: Counter[str]

    def payload(self) -> dict[str, JsonValue]:
        """Only counts and diagnostic codes enter reports, never source text."""
        return {
            "fields": self.fields,
            "translated": self.translated,
            "original": self.fields - self.translated,
            "low_confidence": self.low_confidence,
            "fallback_reasons": dict[str, JsonValue](sorted(self.reasons.items())),
            "pending_parameter_causes": dict[str, JsonValue](
                sorted(self.pending.items())
            ),
        }


def owner_fields(db: Database) -> Iterator[OwnerField]:
    """Enumerate every Japanese main field and section without borrowing another owner."""
    units = {row.values["id"]: row.values["lang"] for row in db.rows("text_unit")}
    for row in db.rows("face_revision"):
        value = row.values
        owner = FaceRevisionOwner(kind="face_revision", revision_id=str(value["id"]))
        if (
            value["effect_unit_id"] is not None
            and units[value["effect_unit_id"]] == "ja"
        ):
            yield OwnerField(owner=owner, field="effect", ordinal=None)
        for section in db.select(
            "face_text_section",
            db.columns("face_text_section"),
            where={"revision_id": value["id"]},
        ):
            if units[section.values["text_unit_id"]] == "ja":
                yield OwnerField(
                    owner=owner,
                    field="section",
                    ordinal=int(str(section.values["ordinal"])),
                )
    for row in db.rows("printing_face"):
        value = row.values
        printed_owner = PrintingFaceOwner(
            kind="printing_face",
            printing_id=str(value["printing_id"]),
            face_id=str(value["face_id"]),
        )
        if (
            value["printed_effect_unit_id"] is not None
            and units[value["printed_effect_unit_id"]] == "ja"
        ):
            yield OwnerField(owner=printed_owner, field="effect", ordinal=None)
        for section in db.select(
            "printing_text_section",
            db.columns("printing_text_section"),
            where={"printing_id": value["printing_id"], "face_id": value["face_id"]},
        ):
            if units[section.values["text_unit_id"]] == "ja":
                yield OwnerField(
                    owner=printed_owner,
                    field="section",
                    ordinal=int(str(section.values["ordinal"])),
                )


def _authored(db: Database, inputs: Inputs, revision: str) -> dict[str, str]:
    audit = {}
    for path, exact, content in inputs.files:
        identifier = (
            "authored:translations:"
            + digest(canonical([revision, path, digest(exact)]))[7:]
        )
        insert_exact(
            db,
            "source_record",
            {
                "id": identifier,
                "kind": "authored",
                "sha256": digest(exact),
                "authored_path": "authored/" + path,
                "authored_revision": revision,
                "parser_version": "translation-authored-current-v3",
            },
            ("id",),
        )
        for record in shard(content).records:
            audit[record.record_key] = identifier
    return audit


@dataclass(frozen=True)
class Settings:
    sources: Sources
    vocabulary: Vocabulary
    rules: Rules
    batch_id: str
    authored_revision: str
    lang: str


@dataclass(frozen=True)
class _Plan:
    fields: tuple[CompiledField, ...]
    canonical_sources: dict[str, str]
    total: int
    reasons: Counter[str]
    pending: Counter[str]


def _confirmed(db: Database, field: OwnerField) -> bool:
    owner = field.owner
    if isinstance(owner, FaceRevisionOwner):
        revision = db.select(
            "face_revision", ("face_id",), where={"id": owner.revision_id}
        )[0]
        face = db.select(
            "face", ("card_id",), where={"id": revision.values["face_id"]}
        )[0]
        card_id = face.values["card_id"]
    elif isinstance(owner, PrintingFaceOwner):
        printing = db.select("printing", ("card_id",), where={"id": owner.printing_id})[
            0
        ]
        card_id = printing.values["card_id"]
    else:
        raise TypeError("Japanese effect field requires a card owner")
    card = db.select("card", ("identity_state",), where={"id": card_id})[0]
    return card.values["identity_state"] == "confirmed"


def _compile(
    db: Database,
    frames: Frames,
    classifier: Classifier,
    settings: Settings,
    controls: Controls,
) -> _Plan:
    compiled: list[CompiledField] = []
    canonical_sources: dict[str, str] = {}
    reasons: Counter[str] = Counter()
    pending: Counter[str] = Counter()
    total = 0
    for field in owner_fields(db):
        total += 1
        if not _confirmed(db, field):
            reasons["unconfirmed_identity"] += 1
            continue
        if empty_field(db, field):
            reasons["empty_source_field"] += 1
            continue
        source = descriptor(db, settings.sources, field, settings.batch_id)
        result = compile_card_field(
            db,
            settings.sources,
            source,
            classifier,
            controls.frames_for(source.source_unit_id, source.source_hash, frames),
        )
        controls.verify(result)
        if not result.complete:
            reasons[
                "missing_name_concept"
                if "missing_card_name_concept" in result.issues
                else "unmatched_source_frame"
            ] += 1
            pending.update(result.issues)
            continue
        compiled.append(result)
        for part, match in zip(result.field.parts, result.matches, strict=True):
            assert match is not None
            previous = canonical_sources.setdefault(
                match.frame.id, part.canonical_source
            )
            if previous != part.canonical_source:
                raise ValueError("Frame hash collision across exact owner fields")
    controls.verify_closure()
    return _Plan(tuple(compiled), canonical_sources, total, reasons, pending)


def _populate(
    db: Database, inputs: Inputs, plan: _Plan, settings: Settings
) -> tuple[dict[tuple[str, str], Form], dict[tuple[str, str], SelectedTarget]]:
    definitions = {r.data.id: r for r in inputs.records if isinstance(r, FrameRecord)}
    if definitions.keys() - plan.canonical_sources.keys():
        raise ValueError("Authored frame has no fully verified source owner field")
    insert_raw_sources(db, (use.source for use in settings.sources.uses))
    audit = _authored(db, inputs, settings.authored_revision)
    for identifier, definition in definitions.items():
        write_frame(
            db,
            definition,
            plan.canonical_sources[identifier],
            audit[definition.record_key],
        )
    forms: dict[tuple[str, str], Form] = {}
    selected: dict[tuple[str, str], SelectedTarget] = {}
    for record in inputs.records:
        if isinstance(record, FormRecord):
            write_form(db, record, audit[record.record_key])
            forms[record.data.id, record.data.lang] = Form(
                record.data, record.origin, record.low_confidence
            )
    for record in inputs.records:
        if isinstance(record, (TargetRecord, TargetVariantRecord)):
            write_target(db, record, audit[record.record_key])
            if isinstance(record, TargetRecord):
                selected[record.data.template_id, record.data.lang] = SelectedTarget(
                    record.data.target, record.origin, record.low_confidence
                )
    return forms, selected


def apply(
    db: Database, inputs: Inputs, snapshot: Snapshot, settings: Settings
) -> Report:
    """Source classification and authored matching finish before targets become active."""
    frames = Frames(r.data for r in inputs.records if isinstance(r, FrameRecord))
    classifier = prepare_classifier(
        db, snapshot, settings.sources, settings.vocabulary, settings.rules
    )
    controls = Controls(inputs)
    plan = _compile(db, frames, classifier, settings, controls)
    forms, selected = _populate(db, inputs, plan, settings)
    renderer = Renderer(
        labels(db, settings.lang), {}, forms, domains=classifier.domains
    )
    controls.prepare_labels(db, classifier, settings.sources)
    translated = low = 0
    for field in plan.fields:
        chosen = controls.select(field, renderer, selected, settings.lang)
        result = render_field(
            db,
            field,
            chosen.renderer,
            chosen.targets,
            settings.lang,
            suppress=chosen.suppress,
        )
        if result.rendered is None:
            plan.reasons.update(result.issues)
        else:
            translated += 1
            low += result.rendered.low_confidence
    return Report(plan.total, translated, low, plan.reasons, plan.pending)
