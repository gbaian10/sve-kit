"""Project authored frames only after replaying this build's exact Japanese owner fields."""

from collections import Counter
from dataclasses import dataclass
from dataclasses import field as dataclass_field
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


@dataclass
class FieldReport:
    field: OwnerField
    translated: bool = False
    low_confidence: bool | None = None
    source_hash: str | None = None
    reasons: tuple[str, ...] = ()
    pending: tuple[str, ...] = ()
    pending_semantics: tuple[str, ...] = ()
    projections: Counter[str] = dataclass_field(default_factory=Counter)
    target_bindings: int = 0
    np_target_bindings: int = 0

    def payload(self) -> dict[str, JsonValue]:
        """Missing sources and pending semantics remain visible even on untranslated fields."""
        return {
            **self.field.model_dump(mode="json"),
            "translated": self.translated,
            "low_confidence": self.low_confidence,
            "source_hash": self.source_hash,
            "reasons": list(self.reasons),
            "pending": list(self.pending),
            "pending_semantics": list(self.pending_semantics),
            "projections": dict[str, JsonValue](sorted(self.projections.items())),
            "target_bindings": self.target_bindings,
            "np_target_bindings": self.np_target_bindings,
        }


@dataclass(frozen=True)
class Report:
    fields: int
    translated: int
    low_confidence: int
    reasons: Counter[str]
    pending: Counter[str]
    unused_frames: tuple[str, ...]
    details: tuple[FieldReport, ...]

    def payload(self) -> dict[str, JsonValue]:
        """Only counts, IDs and diagnostic codes enter reports, never source text."""
        return {
            "unused_authored_frames": [
                {
                    "frame_id": identifier,
                    "reason": "no_fully_verified_source_owner_field",
                }
                for identifier in self.unused_frames
            ],
            "owner_fields": [d.payload() for d in self.details],
            "np_target_bindings": sum(d.np_target_bindings for d in self.details),
            "target_bindings": sum(d.target_bindings for d in self.details),
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
    details: dict[bytes, FieldReport]
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
    details: dict[bytes, FieldReport] = {}
    for field in owner_fields(db):
        total += 1
        detail = FieldReport(field)
        details[canonical(field.model_dump(mode="json"))] = detail
        if not _confirmed(db, field):
            reasons["unconfirmed_identity"] += 1
            detail.reasons = ("unconfirmed_identity",)
            continue
        if empty_field(db, field):
            reasons["empty_source_field"] += 1
            detail.reasons = ("empty_source_field",)
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
        detail.source_hash = source.source_hash
        detail.pending = result.issues
        detail.projections = Counter(
            m.frame.projection.projection_kind for m in result.matches if m is not None
        )
        detail.pending_semantics = tuple(
            m.frame.id
            for m in result.matches
            if m is not None and m.frame.semantic_variant.state == "pending"
        )
        if not result.complete:
            reasons[
                "missing_name_concept"
                if "missing_card_name_concept" in result.issues
                else "unmatched_source_frame"
            ] += 1
            pending.update(result.issues)
            detail.reasons = (
                "missing_name_concept"
                if "missing_card_name_concept" in result.issues
                else "unmatched_source_frame",
            )
            continue
        compiled.append(result)
        for part, match in zip(result.field.parts, result.matches, strict=True):
            assert match is not None
            previous = canonical_sources.setdefault(
                match.frame.id, part.canonical_source
            )
            if previous != part.canonical_source:
                raise ValueError(
                    f"Frame hash collision across exact owner fields: frame_id={match.frame.id}, source_unit_id={source.source_unit_id}"
                )
    controls.verify_closure()
    return _Plan(tuple(compiled), canonical_sources, total, details, reasons, pending)


def _populate(
    db: Database, inputs: Inputs, plan: _Plan, settings: Settings
) -> tuple[dict[tuple[str, str], Form], dict[tuple[str, str], SelectedTarget]]:
    definitions = {r.data.id: r for r in inputs.records if isinstance(r, FrameRecord)}
    insert_raw_sources(db, (use.source for use in settings.sources.uses))
    audit = _authored(db, inputs, settings.authored_revision)
    for identifier, definition in definitions.items():
        if identifier not in plan.canonical_sources:
            continue
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
            if record.data.template_id not in plan.canonical_sources:
                continue
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
        field_key = canonical(
            field.source.descriptor.model_dump(
                mode="json", include={"owner", "field", "ordinal"}
            )
        )
        detail = plan.details[field_key]
        selected_targets = [
            chosen.targets.get((match.frame.id, settings.lang))
            for match in field.matches
            if match is not None
        ]
        detail.target_bindings = sum(t is not None for t in selected_targets)
        detail.np_target_bindings = sum(
            t is not None and any(n.kind == "NP" for n in t.target.nodes)
            for t in selected_targets
        )
        result = render_field(
            db,
            field,
            chosen.renderer,
            chosen.targets,
            settings.lang,
            suppress=chosen.suppress,
        )
        detail.reasons = result.issues
        if result.rendered is None:
            plan.reasons.update(result.issues)
        else:
            detail.translated = True
            detail.low_confidence = result.rendered.low_confidence
            translated += 1
            low += result.rendered.low_confidence
    unused = tuple(
        sorted(
            {r.data.id for r in inputs.records if isinstance(r, FrameRecord)}
            - plan.canonical_sources.keys()
        )
    )
    return Report(
        plan.total,
        translated,
        low,
        plan.reasons,
        plan.pending,
        unused,
        tuple(value for _, value in sorted(plan.details.items())),
    )
