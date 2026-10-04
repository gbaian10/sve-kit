"""Project validated templates and their complete fields into the current build schema."""

from typing import TYPE_CHECKING

from sve_carddb.build_db import Json
from sve_carddb.build_db.rows import insert_exact
from sve_carddb.build_db.t2_translation import OWNERS
from sve_carddb.snapshot.values import canonical, digest, parse
from sve_carddb.template_translations.current_models import (
    DefinitionRecord,
    TranslationRecord,
    VariantRecord,
)
from sve_carddb.template_translations.current_render import Label, Result, render
from sve_carddb.translations.name_sources import NameOwner, _row

if TYPE_CHECKING:
    from sve_carddb.build_db import Database, Value
    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.template_translations.current import Validated
    from sve_carddb.template_translations.current_render import Rendered


def populate(db: Database, validated: Validated) -> None:
    """The caller runs full validation once, before any current template rows are written."""
    members = {m.entry.id: m for m in validated.members}
    audit = {}
    for path, exact, content in validated.inputs.files.content:
        if not path.startswith("translations/templates/"):
            continue
        identifier = (
            "authored:templates:"
            + digest(canonical([validated.inputs.files.revision, path, digest(exact)]))[
                7:
            ]
        )
        insert_exact(
            db,
            "source_record",
            {
                "id": identifier,
                "kind": "authored",
                "sha256": digest(exact),
                "authored_path": "authored/" + path,
                "authored_revision": validated.inputs.files.revision,
                "parser_version": "template-authored-current-v2",
            },
            ("id",),
        )
        from sve_carddb.template_translations.current import shard  # ruff: ignore[import-outside-top-level] -- indexed records locate actual authored provenance

        for record in shard(content).records:
            audit[record.record_key] = identifier
    for record in validated.inputs.records:
        source = audit.get(record.record_key)
        if source is None:
            raise ValueError(
                "Current template projection requires indexed authored provenance"
            )
        values: dict[str, Value] = {
            "authored_source_id": source,
            "record_key": record.record_key,
            "origin": record.origin,
            "low_confidence": record.low_confidence,
        }
        if isinstance(record, DefinitionRecord):
            data = record.data
            values.update(
                id=data.id,
                level="clause" if data.id.startswith("C") else "sentence",
                source_lang=data.source_lang,
                normalized_text=members[data.inventory_id].normalized,
                normalizer_version=data.normalizer_version,
                semantic_variant=data.semantic_variant,
                parameter_schema=Json(data.parameter_schema.model_dump(mode="json")),
                content_hash=data.content_hash,
                supersedes_id=data.supersedes_id,
            )
            insert_exact(db, "sentence_template", values, ("id",))
    for record in validated.inputs.records:
        if isinstance(record, (TranslationRecord, VariantRecord)):
            insert_exact(
                db,
                "template_translation",
                {
                    "template_id": record.data.template_id,
                    "lang": record.data.lang,
                    "variant_key": record.data.variant_key
                    if isinstance(record, VariantRecord)
                    else "default",
                    "text": record.data.text,
                    "authored_source_id": audit[record.record_key],
                    "record_key": record.record_key,
                    "origin": record.origin,
                    "low_confidence": record.low_confidence,
                },
                ("template_id", "lang", "variant_key"),
            )


def labels(db: Database, lang: str) -> tuple[Label, ...]:
    """Read the already validated shared glossary and catalog, not a second word list."""
    result = []
    for choice in db.select(
        "glossary_translation", db.columns("glossary_translation"), where={"lang": lang}
    ):
        value = choice.values
        term = _row(db, "glossary_term", str(value["term_id"]))
        result.append(
            Label(
                "term",
                str(value["term_id"]),
                lang,
                str(value["text"]),
                str(value["origin"]),
                bool(value["low_confidence"] or term["low_confidence"]),
                term["emphasis"] if isinstance(term["emphasis"], bool) else None,
            )
        )
    for vocabulary in db.select("vocabulary", db.columns("vocabulary")):
        value = vocabulary.values
        if not value["active"]:
            continue
        unit = _row(db, "text_unit", str(value["label_unit_id"]))
        if unit["lang"] != lang:
            continue
        result.append(
            Label(
                "vocabulary",
                str(value["kind"]) + ":" + str(value["code"]),
                lang,
                str(unit["text"]),
                str(value["origin"] or "project"),
                bool(value["low_confidence"]),
                None,
            )
        )
    return tuple(result)


def populate_field(  # ruff: ignore[too-many-arguments] -- source, physical owner, field and locale independently constrain one rendered use
    db: Database,
    validated: Validated,
    ref: SourceRef,
    owner: NameOwner,
    field: str,
    lang: str,
    *,
    ordinal: int | None = None,
    variants: tuple[tuple[str, str], ...] = (),
) -> Result:
    """Recheck each actual owner; equal shared text never bypasses printed-field eligibility."""
    source = _source(db, owner, field, ordinal)
    if source is None:
        return Result(None, ("unknown_owner_source",))
    unit_id, text = source
    ending = (
        f"/sections/{ordinal}"
        if field == "section"
        else "/" + ("text" if field == "effect" else field)
    )
    if not ref.locator.endswith(ending) or ref.text_hash != digest(text.encode()):
        raise ValueError("Template use must match its exact owner field and source")
    if field == "flavor" and not any(
        m.entry.source_ref == ref
        and m.owner is not None
        and m.owner.printing_id == owner.identifier
        and m.owner.face_id == owner.face_id
        for m in validated.members
    ):
        raise ValueError("Flavor template use requires its own verified printing owner")
    context_id = (
        "ctx:"
        + digest(
            canonical(
                {
                    "recipe": "context-v1",
                    "source_unit_id": unit_id,
                    "semantic_variant": "default",
                }
            )
        )[7:]
    )
    result = render(
        validated, ref, context_id, text, lang, labels(db, lang), variants=variants
    )
    if result.rendered is None:
        return result
    insert_exact(
        db,
        "translation_context",
        {"id": context_id, "source_unit_id": unit_id, "semantic_variant": "default"},
        ("id",),
    )
    use_id = (
        "use:"
        + digest(
            canonical(
                {
                    "recipe": "use-v1",
                    "owner": owner.payload(),
                    "field": field,
                    "ordinal": ordinal,
                    "context_id": context_id,
                }
            )
        )[7:]
    )
    values: dict[str, Value] = {name: None for group in OWNERS for name in group}
    values.update(id=use_id, context_id=context_id, field=field, ordinal=ordinal)
    if owner.kind == "face_revision":
        values["face_revision_id"] = owner.identifier
    else:
        values.update(printing_id=owner.identifier, face_id=owner.face_id)
    insert_exact(db, "translation_use", values, ("id",))
    _rendered(db, result.rendered)
    return result


def _source(  # ruff: ignore[complex-structure] -- each physical owner branch reads only its own known fields
    db: Database, owner: NameOwner, field: str, ordinal: int | None
) -> tuple[str, str] | None:
    if (field == "section") != (ordinal is not None) or field not in {
        "effect",
        "flavor",
        "section",
    }:
        raise ValueError("Unsupported template owner field or ordinal")
    if owner.kind == "face_revision":
        if field == "flavor":
            raise ValueError("Face revision cannot own flavor text")
        row = _row(db, "face_revision", owner.identifier)
        card = _row(db, "face", str(row["face_id"]))["card_id"]
        unit = row["effect_unit_id"]
        query: dict[str, Value] = {"revision_id": owner.identifier, "ordinal": ordinal}
        table = "face_text_section"
        region = row["region"]
    else:
        printing = _row(db, "printing", owner.identifier)
        rows = db.select(
            "printing_face",
            db.columns("printing_face"),
            where={"printing_id": owner.identifier, "face_id": owner.face_id},
        )
        if len(rows) != 1:
            raise ValueError("Template printing face is absent")
        row = dict(rows[0].values)
        if (
            row["card_id"] != printing["card_id"]
            or _row(db, "face", str(owner.face_id))["card_id"] != printing["card_id"]
        ):
            raise ValueError("Template printing face belongs to another card")
        if row["printed_text_state"] in {"unknown", "omitted"}:
            return None
        card = printing["card_id"]
        unit = (
            row["flavor_unit_id"]
            if field == "flavor"
            else row["printed_effect_unit_id"]
        )
        table = "printing_text_section"
        query = {
            "printing_id": owner.identifier,
            "face_id": owner.face_id,
            "ordinal": ordinal,
        }
        region = printing["region"]
    if _row(db, "card", str(card))["identity_state"] != "confirmed":
        return None
    if field == "section":
        sections = db.select(table, db.columns(table), where=query)
        if len(sections) != 1:
            raise ValueError("Template owner section is absent")
        unit = sections[0].values["text_unit_id"]
    if unit is None:
        return None
    original = _row(db, "text_unit", str(unit))
    text = original["text"]
    if (
        not isinstance(text, str)
        or original["content_hash"] != digest(text.encode())
        or original["lang"] != {"jp": "ja", "en": "en"}.get(str(region))
    ):
        raise ValueError("Template owner text has invalid exact hash or language")
    return str(unit), text


def _rendered(db: Database, rendered: Rendered) -> None:
    translation_id, revision = rendered.identity()
    for binding in rendered.bindings:
        insert_exact(
            db,
            "text_template_binding",
            {
                "id": binding.identifier,
                "context_id": rendered.context_id,
                "ordinal": binding.ordinal,
                "template_id": binding.definition.data.id,
                "params": Json(parse(binding.params)),
                "source_span": Json(binding.source_span()),
            },
            ("id",),
        )
    insert_exact(
        db,
        "translation",
        {
            "id": translation_id,
            "context_id": rendered.context_id,
            "target_lang": rendered.target_lang,
            "revision": revision,
            "text": rendered.text,
            "tokens": None,
            "origin": rendered.origin,
            "authority": "unofficial",
            "low_confidence": rendered.low_confidence,
            "source_hash": rendered.source_hash,
            "source_id": None,
        },
        ("id",),
    )
    for binding, target in zip(rendered.bindings, rendered.templates, strict=True):
        insert_exact(
            db,
            "translation_binding",
            {
                "translation_id": translation_id,
                "binding_id": binding.identifier,
                "template_id": target.data.template_id,
                "lang": target.data.lang,
                "variant_key": target.data.variant_key
                if isinstance(target, VariantRecord)
                else "default",
            },
            ("translation_id", "binding_id"),
        )
    for term_id in sorted(
        {
            use.label.identifier
            for use in rendered.references
            if use.label.kind == "term"
        }
    ):
        insert_exact(
            db,
            "translation_term",
            {"translation_id": translation_id, "term_id": term_id},
            ("translation_id", "term_id"),
        )
    insert_exact(
        db,
        "translation_selection",
        {
            "context_id": rendered.context_id,
            "target_lang": rendered.target_lang,
            "translation_id": translation_id,
        },
        ("context_id", "target_lang"),
    )
