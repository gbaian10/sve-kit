"""Rebuild occurrence-scoped translations for verified, unchanged corrected wording."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField, hash_payload
from sve_carddb.domains.translations.four_layer_authored import (
    FrameRecord,
    Inputs,
    TargetRecord,
    TargetVariantRecord,
)
from sve_carddb.domains.translations.four_layer_matching import Frames
from sve_carddb.domains.translations.four_layer_pipeline import compile_card_field
from sve_carddb.domains.translations.four_layer_sources import descriptor

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.domains.translations.four_layer_authored import Record
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_pipeline import CompiledField
    from sve_carddb.domains.translations.sources import Sources


@dataclass(frozen=True)
class Inherited:
    inputs: Inputs
    records: dict[str, str]
    frames: dict[str, str]


def _same_revision(
    db: Database, before: Mapping[str, Value], after: Mapping[str, Value]
) -> bool:
    provenance = {
        "id",
        "revision",
        "observed_at",
        "change_kind",
        "source_id",
        "supersedes_id",
    }
    if not all(
        before[key] == value for key, value in after.items() if key not in provenance
    ):
        return False
    for table, column in (
        ("face_trait", "trait_code"),
        ("face_title", "title_code"),
        ("face_special_kind", "special_kind_code"),
    ):
        if {
            r.values[column]
            for r in db.select(table, (column,), where={"revision_id": before["id"]})
        } != {
            r.values[column]
            for r in db.select(table, (column,), where={"revision_id": after["id"]})
        }:
            return False
    return True


def _rekey(compiled: CompiledField, owner: FaceRevisionOwner) -> dict[str, Frame]:
    frames = {}
    for part, match in zip(compiled.field.parts, compiled.matches, strict=True):
        assert match is not None
        frame = match.frame
        scope = frame.semantic_variant.scope
        if scope is None:
            continue
        updated = frame.model_copy(
            update={
                "semantic_variant": frame.semantic_variant.model_copy(
                    update={"scope": scope.model_copy(update={"owner": owner})}
                )
            }
        )
        checksum = hash_payload(updated.payload(part.canonical_source))
        frames[frame.id] = updated.model_copy(
            update={"id": "frame:" + checksum, "content_hash": checksum}
        )
    return frames


def _records(inputs: Inputs, frames: Mapping[str, Frame]) -> Inherited:
    added: list[Record] = []
    origins = {}
    for record in inputs.records:
        updated: FrameRecord | TargetRecord | TargetVariantRecord
        if isinstance(record, FrameRecord) and record.data.id in frames:
            updated = record.model_copy(update={"data": frames[record.data.id]})
        elif (
            isinstance(record, (TargetRecord, TargetVariantRecord))
            and record.data.template_id in frames
        ):
            updated = record.model_copy(
                update={
                    "data": record.data.model_copy(
                        update={"template_id": frames[record.data.template_id].id}
                    )
                }
            )
        else:
            continue
        origins[updated.record_key] = record.record_key
        added.append(updated)
    return Inherited(
        Inputs(inputs.files, (*inputs.records, *added)),
        origins,
        {frame.id: original for original, frame in frames.items()},
    )


def _prior(db: Database, revision: Mapping[str, Value]) -> list[Mapping[str, Value]]:
    face = db.select("face", ("card_id",), where={"id": revision["face_id"]})[0].values
    if (
        db.select("card", ("identity_state",), where={"id": face["card_id"]})[0].values[
            "identity_state"
        ]
        != "confirmed"
    ):
        return []
    return sorted(
        (
            r.values
            for r in db.select(
                "face_revision",
                db.columns("face_revision"),
                where={"face_id": revision["face_id"], "region": "jp"},
            )
            if int(str(r.values["revision"])) < int(str(revision["revision"]))
            and _same_revision(db, r.values, revision)
        ),
        key=lambda r: int(str(r["revision"])),
        reverse=True,
    )


def inherit(
    db: Database,
    inputs: Inputs,
    classifier: Classifier,
    sources: Sources,
    batch_id: str,
) -> Inherited:
    """Only an earlier identical revision on this face can donate pending target wording."""
    automatic = Frames(r.data for r in inputs.records if isinstance(r, FrameRecord))
    definitions = {
        r.data.id: r.data for r in inputs.records if isinstance(r, FrameRecord)
    }
    inherited = Inherited(inputs, {}, {})
    for row in db.select(
        "face_revision",
        db.columns("face_revision"),
        where={"region": "jp", "change_kind": "source_correction"},
    ):
        revision = row.values
        prior = _prior(db, revision)
        if not prior:
            continue
        owner = FaceRevisionOwner(kind="face_revision", revision_id=str(revision["id"]))
        fields = [OwnerField(owner=owner, field="effect", ordinal=None)]
        fields.extend(
            OwnerField(
                owner=owner, field="section", ordinal=int(str(r.values["ordinal"]))
            )
            for r in db.select(
                "face_text_section",
                db.columns("face_text_section"),
                where={"revision_id": revision["id"]},
            )
        )
        for field in fields:
            source = descriptor(db, sources, field, batch_id)
            if compile_card_field(db, sources, source, classifier, automatic).complete:
                continue
            for donor in prior:
                donor_field = field.model_copy(
                    update={
                        "owner": FaceRevisionOwner(
                            kind="face_revision", revision_id=str(donor["id"])
                        )
                    }
                )
                if field.field == "section" and not db.select(
                    "face_text_section",
                    ("ordinal",),
                    where={"revision_id": donor["id"], "ordinal": field.ordinal},
                ):
                    continue
                donor_source = descriptor(db, sources, donor_field, batch_id)
                if donor_source.source_hash != source.source_hash:
                    continue
                compiled = compile_card_field(
                    db, sources, donor_source, classifier, automatic
                )
                if not compiled.complete:
                    continue
                frames = {
                    old: new
                    for old, new in _rekey(compiled, owner).items()
                    if new.id not in definitions
                }
                candidates = Frames(
                    (
                        *definitions.values(),
                        *frames.values(),
                    )
                )
                if not compile_card_field(
                    db, sources, source, classifier, candidates
                ).complete:
                    continue
                additions = _records(inputs, frames)
                existing = {r.record_key for r in inherited.inputs.records}
                inherited = Inherited(
                    Inputs(
                        inputs.files,
                        (
                            *inherited.inputs.records,
                            *(
                                r
                                for r in additions.inputs.records
                                if r.record_key not in existing
                            ),
                        ),
                    ),
                    inherited.records | additions.records,
                    inherited.frames | additions.frames,
                )
                break
    return inherited
