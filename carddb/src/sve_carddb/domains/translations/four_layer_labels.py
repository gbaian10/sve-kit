"""Select exact presentation dependencies from the already adopted build tables."""

from typing import TYPE_CHECKING, Literal

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    GlossaryReference,
    VocabularyReference,
)
from sve_carddb.core.json import canonical, string
from sve_carddb.domains.translations.four_layer_render import Label
from sve_carddb.domains.translations.four_layer_storage import payload

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build import Database


def _origin(value: JsonValue) -> Literal["official", "project", "machine"]:
    if value == "official":
        return "official"
    if value == "project":
        return "project"
    if value == "machine":
        return "machine"
    raise ValueError("Selected four-layer label has an invalid origin")


def _emphasis(value: JsonValue) -> bool | None:
    if value is None or isinstance(value, bool):
        return value
    raise TypeError("Selected rule-term emphasis must be a nullable boolean")


def labels(db: Database, lang: str) -> dict[tuple[bytes, str], Label]:
    """Glossary categories set bold independently of spelling, frequency and translation quality."""
    terms = {
        string(value["id"]): value
        for row in db.rows("glossary_term")
        if (value := payload(row))
    }
    result = {}
    for row in db.select(
        "glossary_translation", db.columns("glossary_translation"), where={"lang": lang}
    ):
        choice = payload(row)
        term = terms[string(choice["term_id"])]
        reference = (
            CardNameReference(kind="card_name", term_id=string(term["id"]))
            if term["category"] == "card_name"
            else GlossaryReference(kind="glossary", key=string(term["id"]))
        )
        selected = Label(
            reference,
            lang,
            string(choice["text"]),
            _origin(choice["origin"]),
            bool(choice["low_confidence"] or term["low_confidence"]),
            _emphasis(term["emphasis"]) if term["category"] == "rule_term" else True,
        )
        result[canonical(reference.model_dump(mode="json")), lang] = selected
    selected_vocabulary = _vocabulary(db, lang)
    units = {
        string(value["id"]): value
        for row in db.rows("text_unit")
        if (value := payload(row))
    }
    for row in db.rows("vocabulary"):
        term = payload(row)
        if not term["active"]:
            continue
        kind, code = string(term["kind"]), string(term["code"])
        vocabulary_reference = VocabularyReference(kind="vocabulary", key=(kind, code))
        original = units[string(term["label_unit_id"])]
        if original["lang"] == lang:
            text, origin, low = (
                string(original["text"]),
                _origin(term["origin"] or "project"),
                bool(term["low_confidence"]),
            )
        elif (kind, code) in selected_vocabulary:
            choice = selected_vocabulary[kind, code]
            text, origin, low = (
                string(choice["text"]),
                _origin(choice["origin"]),
                bool(choice["low_confidence"] or term["low_confidence"]),
            )
        else:
            continue
        result[canonical(vocabulary_reference.model_dump(mode="json")), lang] = Label(
            vocabulary_reference,
            lang,
            text,
            origin,
            low,
            True if kind in {"class", "type"} else None,
        )
    return result


def _vocabulary(db: Database, lang: str) -> dict[tuple[str, str], dict[str, JsonValue]]:
    translations = {
        string(value["id"]): value
        for row in db.rows("translation")
        if (value := payload(row))
    }
    selected = {
        string(value["context_id"]): translations[string(value["translation_id"])]
        for row in db.rows("translation_selection")
        if (value := payload(row)) and value["target_lang"] == lang
    }
    return {
        (string(use["vocabulary_kind"]), string(use["vocabulary_code"])): selected[
            string(use["context_id"])
        ]
        for row in db.rows("translation_use")
        if (use := payload(row))
        and use["field"] == "label"
        and use["vocabulary_kind"] is not None
        and string(use["context_id"]) in selected
    }
