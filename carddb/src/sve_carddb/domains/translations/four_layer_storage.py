"""Typed relational reads reconstruct exact arrays before checking four-layer identities."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.build import Json
from sve_carddb.contracts.annotations import AnnotationSet
from sve_carddb.contracts.four_layer import (
    CardNameReference,
    FormDefinition,
    Frame,
    OwnerField,
    VocabularyReference,
)
from sve_carddb.core.json import canonical, string
from sve_carddb.domains.translations.four_layer_authored import (
    FormRecord,
    FrameRecord,
    TargetRecord,
    TargetVariantRecord,
)
from sve_carddb.domains.translations.four_layer_normalizer import VERSION, replay_part
from sve_carddb.domains.translations.four_layer_render import validate_form

if TYPE_CHECKING:
    from collections.abc import Mapping

    from pydantic import JsonValue

    from sve_carddb.build import Database, Row, Value
    from sve_carddb.contracts.source_binding import ClosedDomain, SourceBinding


@dataclass(frozen=True)
class StoredFrame:
    frame: Frame
    canonical_source: str


def write_frame(
    db: Database, record: FrameRecord, canonical_source: str, authored_source_id: str
) -> None:
    """Content-addressed source semantics are checked before relational projection."""
    frame = record.data
    frame.verify(canonical_source)
    if frame.source.normalizer_version != VERSION:
        raise ValueError("Unsupported stored normalizer version")
    values: dict[str, Value] = {
        "canonical_source": canonical_source,
        "interface_key": frame.projection.interface_key(frame.id),
        "authored_source_id": authored_source_id,
        "record_key": record.record_key,
        "origin": record.origin,
        "low_confidence": record.low_confidence,
    }
    for key, value in frame.model_dump(mode="json").items():
        if key in {"source", "semantic_variant", "leaf_schema", "projection"}:
            values[key] = Json(value)
        elif isinstance(value, str):
            values[key] = value
        else:
            raise TypeError("Unexpected scalar frame column")
    db.insert("sentence_template", values)


def write_form(db: Database, record: FormRecord, authored_source_id: str) -> None:
    """Editable cases retain the pinned rule and signature on writes as well as reads."""
    validate_form(record.data)
    db.insert(
        "translation_form",
        {
            "id": record.data.id,
            "lang": record.data.lang,
            "rule": record.data.rule,
            "signature": Json(
                [arg.model_dump(mode="json") for arg in record.data.signature]
            ),
            "cases": Json(
                {
                    key: [part.model_dump(mode="json") for part in parts]
                    for key, parts in record.data.cases.items()
                }
            ),
            "authored_source_id": authored_source_id,
            "record_key": record.record_key,
            "origin": record.origin,
            "low_confidence": record.low_confidence,
        },
    )


def write_target(
    db: Database,
    record: TargetRecord | TargetVariantRecord,
    authored_source_id: str,
    *,
    frame: Frame,
    forms: Mapping[tuple[str, str], FormDefinition],
) -> None:
    """Validate targets against the frame and forms already checked on their writes."""
    if frame.id != record.data.template_id:
        raise ValueError(
            "Target template differs from the frame it is verified against"
        )
    if frame.source.source_lang == record.data.lang:
        raise ValueError("Stored target language must differ from source language")
    record.data.target.verify(frame.leaf_schema, record.data.lang, forms)
    db.insert(
        "template_translation",
        {
            "template_id": frame.id,
            "lang": record.data.lang,
            "variant_key": record.data.variant_key
            if isinstance(record, TargetVariantRecord)
            else "default",
            "target": Json(record.data.target.model_dump(mode="json")),
            "authored_source_id": authored_source_id,
            "record_key": record.record_key,
            "origin": record.origin,
            "low_confidence": record.low_confidence,
        },
    )


def payload(row: Row) -> dict[str, JsonValue]:
    """SQLite storage classes already passed the generic boundary; JSON stays explicitly typed."""
    return {
        key: value.value if isinstance(value, Json) else value
        for key, value in row.values.items()
    }


def _one(db: Database, table: str, where: Mapping[str, Value]) -> Row:
    rows = db.select(table, db.columns(table), where=where)
    if len(rows) != 1:
        raise ValueError("Four-layer row requires an exact existing primary key")
    return rows[0]


def read_frame(db: Database, identifier: str) -> StoredFrame:
    """Content and interface identities are checked independently on every typed read."""
    data = payload(_one(db, "sentence_template", {"id": identifier}))
    source = string(data.pop("canonical_source"))
    interface = data.pop("interface_key")
    for name in ("authored_source_id", "record_key", "origin", "low_confidence"):
        data.pop(name)
    try:
        frame = Frame.model_validate_json(canonical(data))
        frame.verify(source)
    except ValueError, TypeError:
        raise ValueError("Invalid stored four-layer frame") from None
    if frame.source.normalizer_version != VERSION:
        raise ValueError("Unsupported stored normalizer version")
    if interface != frame.projection.interface_key(frame.id):
        raise ValueError("Stored frame interface key differs from its projection")
    return StoredFrame(frame, source)


def read_forms(db: Database) -> dict[tuple[str, str], FormDefinition]:
    """Stored editable parts cannot change a pinned rule's branch set or signature."""
    columns = ("id", "lang", "rule", "signature", "cases")
    result = {}
    for row in db.select("translation_form", columns):
        try:
            form = FormDefinition.model_validate_json(canonical(payload(row)))
            validate_form(form)
        except ValueError, TypeError:
            raise ValueError("Invalid stored four-layer form") from None
        result[form.id, form.lang] = form
    return result


def use_field(row: Row) -> OwnerField:
    """The exact translation-use columns select an owner; text equality is insufficient."""
    use = payload(row)
    owners: list[dict[str, JsonValue]] = []
    for column, kind, key in (
        ("face_revision_id", "face_revision", "revision_id"),
        ("qa_version_id", "qa_version", "qa_version_id"),
        ("cr_clause_id", "cr_clause", "cr_clause_id"),
        ("keyword_id", "keyword", "keyword_id"),
        ("product_id", "product", "product_id"),
        ("product_family_id", "product_family", "product_family_id"),
    ):
        if use[column] is not None:
            owners.append({"kind": kind, key: use[column]})
    if use["printing_id"] is not None:
        owners.append(
            {
                "kind": "printing_face",
                "printing_id": use["printing_id"],
                "face_id": use["face_id"],
            }
        )
    if use["vocabulary_kind"] is not None:
        owners.append(
            {
                "kind": "vocabulary",
                "vocabulary_kind": use["vocabulary_kind"],
                "vocabulary_code": use["vocabulary_code"],
            }
        )
    if len(owners) != 1:
        raise ValueError("Stored binding use requires exactly one typed owner")
    return OwnerField.model_validate_json(
        canonical(
            {"owner": owners[0], "field": use["field"], "ordinal": use["ordinal"]}
        )
    )


def _verify_use(db: Database, use_id: str, binding: SourceBinding) -> str:
    use = _one(db, "translation_use", {"id": use_id})
    if OwnerField(
        owner=binding.source.owner,
        field=binding.source.field,
        ordinal=binding.source.ordinal,
    ) != use_field(use):
        raise ValueError("Stored binding source differs from its exact use owner")
    context = _one(db, "translation_context", {"id": use.values["context_id"]})
    if binding.source.source_unit_id != context.values["source_unit_id"]:
        raise ValueError("Stored binding source differs from its exact use context")
    unit = payload(_one(db, "text_unit", {"id": binding.source.source_unit_id}))
    binding.source.verify(binding.source, string(unit["text"]))
    if (
        unit["lang"] != "ja"
        or unit["content_hash"] != "sha256:" + binding.source.source_hash
    ):
        raise ValueError(
            "Stored source unit differs from binding source hash or language"
        )
    return string(unit["text"])


def write_binding(
    db: Database,
    use_id: str,
    binding: SourceBinding,
    domains: Mapping[str, ClosedDomain],
) -> None:
    """Check the full object before splitting its ordered occurrences into rows."""
    stored_frame = read_frame(db, binding.frame_id)
    frame = stored_frame.frame
    binding.verify(frame, domains)
    replay_part(
        _verify_use(db, use_id, binding),
        frame,
        binding,
        domains,
        canonical_source=stored_frame.canonical_source,
    )
    data = binding.model_dump(mode="json")
    occurrences = data.pop("occurrences")
    if not isinstance(occurrences, list):
        raise TypeError("Binding occurrences must serialize as an array")
    values: dict[str, Value] = {"use_id": use_id}
    for key, value in data.items():
        if key in {"source", "source_span", "values", "trace"}:
            values[key] = Json(value)
        elif isinstance(value, (str, int)):
            values[key] = value
        else:
            raise ValueError("Unexpected scalar binding column")
    db.insert("text_template_binding", values)
    for position, occurrence in enumerate(binding.occurrences):
        leaf = occurrence.model_dump(mode="json")
        stored: dict[str, Value] = {"binding_id": binding.id, "position": position}
        for key, value in leaf.items():
            if key in {"raw_spans", "canonical_spans"}:
                stored[key] = Json(value)
            elif value is None or isinstance(value, (str, int)):
                stored[key] = value
            else:
                raise ValueError("Unexpected scalar leaf occurrence column")
        db.insert("binding_leaf_occurrence", stored)


def write_annotation(db: Database, annotation: AnnotationSet) -> None:
    """Annotation identity and concept references are validated before insertion."""
    _verify_annotation(db, annotation)
    db.insert(
        "annotation_set",
        {
            "id": annotation.id,
            "text_unit_id": annotation.text_unit_id,
            "occurrences": Json(
                [a.model_dump(mode="json") for a in annotation.occurrences]
            ),
        },
    )


def _verify_annotation(db: Database, annotation: AnnotationSet) -> None:
    text = payload(_one(db, "text_unit", {"id": annotation.text_unit_id}))
    annotation.verify(string(text["id"]), string(text["lang"]), string(text["text"]))
    for occurrence in annotation.occurrences:
        reference = occurrence.reference
        if isinstance(reference, VocabularyReference):
            _one(db, "vocabulary", {"kind": reference.key[0], "code": reference.key[1]})
            if reference.key[0] in {"class", "type"} and occurrence.bold is not True:
                raise ValueError("Stored class/type annotation must be bold")
        else:
            name = (
                reference.term_id
                if isinstance(reference, CardNameReference)
                else reference.key
            )
            term = _one(db, "glossary_term", {"id": name})
            if (term.values["category"] == "card_name") != isinstance(
                reference, CardNameReference
            ):
                raise ValueError(
                    "Stored annotation reference has the wrong concept category"
                )
            bold = (
                term.values["emphasis"]
                if term.values["category"] == "rule_term"
                else True
            )
            if occurrence.bold is not bold:
                raise ValueError("Stored annotation differs from concept emphasis")


def read_annotation(db: Database, identifier: str) -> AnnotationSet:
    """Exact text identity and scalar positions survive the relational round trip."""
    try:
        result = AnnotationSet.model_validate_json(
            canonical(payload(_one(db, "annotation_set", {"id": identifier})))
        )
        _verify_annotation(db, result)
    except ValueError, TypeError:
        raise ValueError("Invalid stored annotation set") from None
    return result
