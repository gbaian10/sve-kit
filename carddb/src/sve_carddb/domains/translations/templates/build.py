"""Project validated templates and their complete fields into the current build schema."""

from collections import Counter
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build import Json
from sve_carddb.build.rows import insert_exact
from sve_carddb.build.t2_translation import OWNERS
from sve_carddb.core.json import canonical, digest, parse
from sve_carddb.domains.translations.names.sources import NameOwner
from sve_carddb.domains.translations.templates.records import (
    DefinitionRecord,
    TranslationRecord,
    VariantRecord,
)
from sve_carddb.domains.translations.templates.render import Label, render

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.domains.catalog.adoption_models import SourceRef
    from sve_carddb.domains.translations.templates.loader import Validated
    from sve_carddb.domains.translations.templates.render import Rendered


def populate(db: Database, validated: Validated) -> None:
    """The caller runs full validation once, before any current template rows are written."""
    members = {m.entry.id: m for m in validated.members}
    # Every matched position of a definition has the same normalized pattern.
    patterns = {
        identifier: members[entry].normalized for entry, identifier in validated.matches
    }
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
        from sve_carddb.domains.translations.templates.loader import shard  # ruff: ignore[import-outside-top-level] -- indexed records locate actual authored provenance

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
                normalized_text=patterns[data.id],
                normalizer_version=data.normalizer_version,
                semantic_variant=data.semantic_variant,
                parameter_schema=Json(data.parameter_schema.model_dump(mode="json")),
                content_hash=data.content_hash,
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


def labels(db: Database, lang: str) -> dict[tuple[str, str, str], Label]:
    """Read the already validated shared glossary, catalog and names, not a second word list."""
    found: list[Label] = []
    terms = {row.values["id"]: row.values for row in db.rows("glossary_term")}
    for choice in db.select(
        "glossary_translation", db.columns("glossary_translation"), where={"lang": lang}
    ):
        value = choice.values
        term = terms[value["term_id"]]
        found.append(
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
    units = {row.values["id"]: row.values for row in db.rows("text_unit")}
    vocabulary, names = _selected_labels(db, lang, units)
    for row in db.rows("vocabulary"):
        value = row.values
        if not value["active"]:
            continue
        key = str(value["kind"]) + ":" + str(value["code"])
        unit = units[value["label_unit_id"]]
        if unit["lang"] == lang:
            text, origin, low = (
                str(unit["text"]),
                str(value["origin"] or "project"),
                bool(value["low_confidence"]),
            )
        elif key in vocabulary:
            text, origin, low = vocabulary[key]
            low = low or bool(value["low_confidence"])
        else:
            continue
        found.append(Label("vocabulary", key, lang, text, origin, low, None))
    found.extend(
        Label("card_name", source, lang, text, origin, low, None)
        for source, (text, origin, low) in sorted(names.items())
    )
    # Glossary, vocabulary and name keys are primary keys of their own tables.
    return {(label.kind, label.identifier, label.lang): label for label in found}


def _selected_labels(
    db: Database, lang: str, units: Mapping[Value, Mapping[str, Value]]
) -> tuple[dict[str, tuple[str, str, bool]], dict[str, tuple[str, str, bool]]]:
    """Selected vocabulary-label and name translations become reference labels."""
    contexts = {
        row.values["id"]: row.values["source_unit_id"]
        for row in db.rows("translation_context")
    }
    translations = {row.values["id"]: row.values for row in db.rows("translation")}
    selected = {
        row.values["context_id"]: translations[row.values["translation_id"]]
        for row in db.rows("translation_selection")
        if row.values["target_lang"] == lang
    }
    vocabulary: dict[str, tuple[str, str, bool]] = {}
    names: dict[str, tuple[str, str, bool]] = {}
    for row in db.rows("translation_use"):
        use = row.values
        chosen = selected.get(use["context_id"])
        if chosen is None:
            continue
        value = (
            str(chosen["text"]),
            str(chosen["origin"]),
            bool(chosen["low_confidence"]),
        )
        if use["field"] == "label" and use["vocabulary_kind"] is not None:
            vocabulary[
                str(use["vocabulary_kind"]) + ":" + str(use["vocabulary_code"])
            ] = value
        elif use["field"] == "name":
            source = units[contexts[use["context_id"]]]
            if source["lang"] == "ja":
                names[str(source["text"])] = value
    return vocabulary, names


@dataclass(frozen=True)
class Report:
    fields: int
    translated: int
    low_confidence: int
    reasons: Counter[str]
    pending: Counter[str]

    def payload(self) -> dict[str, JsonValue]:
        """Counts and reason codes only; the report never repeats card text."""
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


@dataclass(frozen=True)
class _Field:
    owner: NameOwner
    card_id: Value
    field: str
    ordinal: int | None
    unit_id: str


def apply(db: Database, validated: Validated, lang: str) -> Report:
    """Render each Japanese effect field whose exact source hash a template covers."""
    populate(db, validated)
    application = _Application(db, validated, lang)
    for item in _fields(db):
        application.field(item)
    return application.report()


class _Application:
    def __init__(self, db: Database, validated: Validated, lang: str) -> None:
        self.db, self.validated, self.lang = db, validated, lang
        self.labels = labels(db, lang)
        self.refs: dict[tuple[str, int | None, str], SourceRef] = {}
        for ref in sorted(
            validated.field_members, key=lambda r: (r.source_version_id, r.locator)
        ):
            head, _, section = ref.locator.partition("/sections/")
            key = ("section", int(section)) if head != ref.locator else ("effect", None)
            self.refs.setdefault((*key, ref.text_hash), ref)
        self.units = {row.values["id"]: row.values for row in db.rows("text_unit")}
        self.cards = {row.values["id"]: row.values for row in db.rows("card")}
        self.taken = {
            row.values["context_id"]
            for row in db.rows("translation_selection")
            if row.values["target_lang"] == lang
        }
        self.written: dict[str, tuple[tuple[str, int | None], Rendered]] = {}
        self.reasons: Counter[str] = Counter()
        self.pending: Counter[str] = Counter()
        self.total = self.translated = self.low = 0

    def report(self) -> Report:
        return Report(self.total, self.translated, self.low, self.reasons, self.pending)

    def field(self, item: _Field) -> None:
        """Count every Japanese field; only a complete render replaces the original."""
        unit = self.units[item.unit_id]
        if unit["lang"] != "ja":
            return
        self.total += 1
        text = str(unit["text"])
        if unit["content_hash"] != digest(text.encode()):
            raise ValueError("Template owner text has invalid exact hash")
        ref = self.refs.get((item.field, item.ordinal, str(unit["content_hash"])))
        if self.cards[item.card_id]["identity_state"] != "confirmed":
            self.reasons["unconfirmed_identity"] += 1
        elif ref is None:
            self.reasons["unmatched_template_source"] += 1
        else:
            rendered = self._render(item, ref, text)
            if rendered is not None:
                _use(self.db, item, _context(item.unit_id))
                self.translated += 1
                self.low += rendered.low_confidence

    def _render(self, item: _Field, ref: SourceRef, text: str) -> Rendered | None:
        context_id = _context(item.unit_id)
        previous = self.written.get(item.unit_id)
        if previous is not None and previous[0] == (item.field, item.ordinal):
            return previous[1]
        result = render(self.validated, ref, context_id, text, self.lang, self.labels)
        if result.rendered is None:
            self.reasons[result.issues[0]] += 1
            self.pending.update(result.issues[1:])
            return None
        # One source text has one selection; a differing reading of it stays original.
        if (previous is None and context_id in self.taken) or (
            previous is not None and previous[1].text != result.rendered.text
        ):
            self.reasons["shared_source_translated"] += 1
            return None
        if previous is not None:
            return previous[1]
        insert_exact(
            self.db,
            "translation_context",
            {
                "id": context_id,
                "source_unit_id": item.unit_id,
                "semantic_variant": "default",
            },
            ("id",),
        )
        _rendered(self.db, result.rendered)
        self.written[item.unit_id] = ((item.field, item.ordinal), result.rendered)
        return result.rendered


def _context(unit_id: str) -> str:
    return (
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


def _use(db: Database, item: _Field, context_id: str) -> None:
    use_id = (
        "use:"
        + digest(
            canonical(
                {
                    "recipe": "use-v1",
                    "owner": item.owner.payload(),
                    "field": item.field,
                    "ordinal": item.ordinal,
                    "context_id": context_id,
                }
            )
        )[7:]
    )
    values: dict[str, Value] = {name: None for group in OWNERS for name in group}
    values.update(
        id=use_id, context_id=context_id, field=item.field, ordinal=item.ordinal
    )
    if item.owner.kind == "face_revision":
        values["face_revision_id"] = item.owner.identifier
    else:
        values.update(printing_id=item.owner.identifier, face_id=item.owner.face_id)
    insert_exact(db, "translation_use", values, ("id",))


def _fields(db: Database) -> list[_Field]:
    """Every main text and section of face revisions and printed faces, whatever their state."""
    faces = {row.values["id"]: row.values["card_id"] for row in db.rows("face")}
    result: list[_Field] = []
    sections: dict[tuple[Value, ...], list[tuple[int, Value]]] = {}
    for row in db.rows("face_text_section"):
        value = row.values
        sections.setdefault((value["revision_id"],), []).append(
            (int(str(value["ordinal"])), value["text_unit_id"])
        )
    for row in db.rows("printing_text_section"):
        value = row.values
        sections.setdefault((value["printing_id"], value["face_id"]), []).append(
            (int(str(value["ordinal"])), value["text_unit_id"])
        )
    for row in db.rows("face_revision"):
        value = row.values
        owner = NameOwner("face_revision", str(value["id"]))
        result.extend(
            _owner_fields(
                owner,
                faces[value["face_id"]],
                value["effect_unit_id"],
                sections.get((value["id"],), []),
            )
        )
    for row in db.rows("printing_face"):
        value = row.values
        owner = NameOwner(
            "printing_face", str(value["printing_id"]), str(value["face_id"])
        )
        result.extend(
            _owner_fields(
                owner,
                value["card_id"],
                value["printed_effect_unit_id"],
                sections.get((value["printing_id"], value["face_id"]), []),
            )
        )
    return result


def _owner_fields(
    owner: NameOwner,
    card_id: Value,
    effect: Value,
    sections: list[tuple[int, Value]],
) -> list[_Field]:
    result = (
        [] if effect is None else [_Field(owner, card_id, "effect", None, str(effect))]
    )
    result.extend(
        _Field(owner, card_id, "section", ordinal, str(unit))
        for ordinal, unit in sorted(sections)
        if unit is not None
    )
    return result


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
