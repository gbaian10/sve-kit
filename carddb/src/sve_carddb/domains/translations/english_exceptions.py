"""Select exact English display fields without enabling English rule bindings."""

from collections import Counter
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build.rows import insert_exact
from sve_carddb.contracts.annotations import Annotation, AnnotationSet
from sve_carddb.contracts.four_layer import (
    CardNameReference,
    FaceRevisionOwner,
    GlossaryReference,
    LeafRef,
    LiteralNode,
    Span,
    VocabularyReference,
    hash_payload,
)
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.translations.english_records import (
    EnglishTargetRecord,
    EnglishUseRecord,
)
from sve_carddb.domains.translations.four_layer_fields import write_use
from sve_carddb.domains.translations.four_layer_labels import labels
from sve_carddb.domains.translations.four_layer_render import Rendered
from sve_carddb.domains.translations.four_layer_sources import card_source
from sve_carddb.domains.translations.four_layer_storage import (
    read_annotation,
    write_annotation,
)
from sve_carddb.ingest.archive.source_archive import ArchiveError
from sve_carddb.ingest.http.validate import ValidationError

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database
    from sve_carddb.domains.translations.four_layer_authored import Inputs
    from sve_carddb.domains.translations.four_layer_render import Label
    from sve_carddb.domains.translations.four_layer_sources import CardSource
    from sve_carddb.domains.translations.sources import Sources


def _identity(db: Database, record: EnglishUseRecord) -> str | None:
    use = record.data
    if use.identity != "confirmed_no_jp":
        return "unresolved_english_identity"
    card = db.select("card", ("identity_state",), where={"id": use.card_id})
    if not card or card[0].values["identity_state"] != "confirmed":
        return "unconfirmed_identity"
    if db.select("printing", ("id",), where={"card_id": use.card_id, "region": "jp"}):
        return "has_jp_source"
    if any(
        db.select(
            "face_current",
            ("revision_id",),
            where={"face_id": face.values["id"], "region": "jp"},
        )
        for face in db.select("face", ("id",), where={"card_id": use.card_id})
    ):
        return "has_jp_source"
    reviews = db.select(
        "region_mapping_review",
        ("state", "as_of"),
        where={"card_id": use.card_id, "target_region": "jp"},
    )
    latest = max(reviews, key=lambda row: str(row.values["as_of"]), default=None)
    if latest is None or latest.values["state"] != "confirmed_none":
        return "unresolved_english_identity"
    return None


def _references(db: Database, target: EnglishTargetRecord) -> None:
    for reference in target.data.references.values():
        if isinstance(reference, VocabularyReference):
            rows = db.select(
                "vocabulary",
                ("active",),
                where={"kind": reference.key[0], "code": reference.key[1]},
            )
            if not rows or rows[0].values["active"] is not True:
                raise ValueError("English target references unavailable vocabulary")
        else:
            identifier = (
                reference.term_id
                if isinstance(reference, CardNameReference)
                else reference.key
            )
            rows = db.select("glossary_term", ("category",), where={"id": identifier})
            if not rows or (rows[0].values["category"] == "card_name") != isinstance(
                reference, CardNameReference
            ):
                raise ValueError(
                    "English target references an invalid glossary concept"
                )


def _render(
    use: EnglishUseRecord,
    target: EnglishTargetRecord,
    selected: Mapping[tuple[bytes, str], Label],
    context: str,
) -> Rendered | None:
    text = ""
    annotations: list[Annotation] = []
    dependencies: list[object] = [
        use.origin,
        use.low_confidence,
        target.model_dump(mode="json"),
    ]
    low = use.low_confidence or target.low_confidence
    machine = use.origin == "machine" or target.origin == "machine"
    for node in target.data.nodes:
        if isinstance(node, LiteralNode):
            text += node.text
        elif isinstance(node, LeafRef):
            reference = target.data.references[node.slot]
            label = selected.get(
                (canonical(reference.model_dump(mode="json")), target.data.lang)
            )
            if label is None:
                return None
            start = len(text)
            text += label.text
            low |= label.low_confidence
            machine |= label.origin == "machine"
            dependencies.append(
                [label.text, label.origin, label.low_confidence, label.bold]
            )
            annotations.append(
                Annotation(
                    ordinal=len(annotations),
                    reference=reference,
                    ranges=(Span(start=start, end=len(text)),),
                    bold=label.bold,
                )
            )
    unit_id = "t:" + target.data.lang + ":" + digest(text.encode())[7:23]
    annotation = AnnotationSet(
        id="ann:"
        + hash_payload(
            {
                "recipe": "annotation-v1",
                "text_unit_id": unit_id,
                "occurrences": [a.model_dump(mode="json") for a in annotations],
            }
        ),
        text_unit_id=unit_id,
        occurrences=tuple(annotations),
    )
    annotation.verify(unit_id, target.data.lang, text)
    return Rendered(
        context,
        target.data.lang,
        text,
        annotation,
        hash_payload(dependencies),
        "machine" if machine else "project",
        low,
        (),
    )


def _store(db: Database, use: EnglishUseRecord, rendered: Rendered, audit: str) -> None:
    source = use.data.source
    context = rendered.context_id
    insert_exact(
        db,
        "translation_context",
        {
            "id": context,
            "source_unit_id": source.source_unit_id,
            # Shared targets can have different per-owner quality flags.
            "semantic_variant": "english_exception:" + context.removeprefix("ctx:"),
        },
        ("id",),
    )
    write_use(db, source, context)
    identifier, revision = rendered.identity()
    insert_exact(
        db,
        "translation",
        {
            "id": identifier,
            "context_id": context,
            "target_lang": rendered.target_lang,
            "revision": revision,
            "text": rendered.text,
            "tokens": None,
            "origin": rendered.origin,
            "authority": "unofficial",
            "low_confidence": rendered.low_confidence,
            "source_hash": "sha256:" + source.source_hash,
            "source_id": audit,
        },
        ("id",),
    )
    if rendered.annotation.occurrences:
        insert_exact(
            db,
            "text_unit",
            {
                "id": rendered.annotation.text_unit_id,
                "lang": rendered.target_lang,
                "text": rendered.text,
                "content_hash": digest(rendered.text.encode()),
            },
            ("id",),
        )
        if db.select("annotation_set", ("id",), where={"id": rendered.annotation.id}):
            if read_annotation(db, rendered.annotation.id) != rendered.annotation:
                raise ValueError("English annotation content-address collision")
        else:
            write_annotation(db, rendered.annotation)
        insert_exact(
            db,
            "translation_annotation",
            {"translation_id": identifier, "annotation_set_id": rendered.annotation.id},
            ("translation_id",),
        )
    for annotation in rendered.annotation.occurrences:
        reference = annotation.reference
        if isinstance(reference, (GlossaryReference, CardNameReference)):
            insert_exact(
                db,
                "translation_term",
                {
                    "translation_id": identifier,
                    "term_id": reference.term_id
                    if isinstance(reference, CardNameReference)
                    else reference.key,
                },
                ("translation_id", "term_id"),
            )
    insert_exact(
        db,
        "translation_selection",
        {
            "context_id": context,
            "target_lang": rendered.target_lang,
            "translation_id": identifier,
        },
        ("context_id", "target_lang"),
    )


def _source(
    db: Database, sources: Sources, use: EnglishUseRecord
) -> tuple[CardSource | None, str | None]:
    try:
        source = card_source(db, sources, use.data.source, lang="en")
    except ValueError, ArchiveError, FileNotFoundError, ValidationError:
        return None, "english_source_mismatch"
    if (source.card_id, source.face_id) != (use.data.card_id, use.data.face_id):
        return None, "english_owner_face_mismatch"
    owner = use.data.source.owner
    if isinstance(owner, FaceRevisionOwner) and not db.select(
        "face_current",
        ("face_id",),
        where={
            "face_id": source.face_id,
            "region": "en",
            "revision_id": owner.revision_id,
        },
    ):
        return None, "english_source_not_current"
    return source, None


def apply(
    db: Database, inputs: Inputs, sources: Sources, audit: Mapping[str, str]
) -> dict[str, JsonValue] | None:
    """Stale sources and unresolved identities retain the complete English original."""
    targets = {
        r.data.id: r for r in inputs.records if isinstance(r, EnglishTargetRecord)
    }
    uses = tuple(r for r in inputs.records if isinstance(r, EnglishUseRecord))
    if not targets and not uses:
        return None
    for target in targets.values():
        _references(db, target)
    selected = labels(db, "zh-Hant")
    reasons: Counter[str] = Counter()
    translated = low = 0
    for use in uses:
        reason = _identity(db, use)
        if reason is not None:
            reasons[reason] += 1
            continue
        source, reason = _source(db, sources, use)
        if reason is not None:
            reasons[reason] += 1
            continue
        assert source is not None
        target = targets[use.data.target_id]
        context = "ctx:" + hash_payload(
            [source.descriptor.model_dump(mode="json"), target.data.id]
        )
        rendered = _render(use, target, selected, context)
        if rendered is None:
            reasons["missing_english_reference_translation"] += 1
            continue
        _store(db, use, rendered, audit[use.record_key])
        translated += 1
        low += rendered.low_confidence
    return {
        "fields": len(uses),
        "translated": translated,
        "original": len(uses) - translated,
        "low_confidence": low,
        "fallback_reasons": dict[str, JsonValue](sorted(reasons.items())),
    }
