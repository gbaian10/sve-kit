"""Reusable lexical checks against this build's adopted glossary, without card text logs."""

import re
import unicodedata
from collections import Counter
from typing import TYPE_CHECKING

from sve_carddb.domains.translations.qa import sources

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.domains.translations.qa import Entry, Key

NAMES = re.compile(r"『([^』]+)』")
NUMBERS = re.compile(r"Q[0-9]+|[0-9]+")
# Style puts the object between the verb and the destination, and glossary values elide objects with "…".
LOOSE = {"召喚到戰場上": "召喚.*到戰場上"}
OPENINGS = {
    "はい。": ("是的。",),
    "いいえ。": ("不行。", "不會。", "不是。", "不需要。"),
}


def vocabulary(db: Database, *, names: bool) -> dict[str, set[str]]:
    """Use adopted values already resolved from the authored glossary and frozen evidence."""
    if names:
        units = {r.values["id"]: r.values for r in db.rows("text_unit")}
        contexts = {
            r.values["id"]: r.values["source_unit_id"]
            for r in db.rows("translation_context")
        }
        owners = {
            r.values["context_id"]
            for r in db.rows("translation_use")
            if r.values["field"] == "name"
        }
        result: dict[str, set[str]] = {}
        for row in db.rows("translation"):
            if (
                row.values["context_id"] not in owners
                or row.values["target_lang"] != "zh-Hant"
            ):
                continue
            unit = units[contexts[row.values["context_id"]]]
            if unit["lang"] == "ja":
                result.setdefault(str(unit["text"]), set()).add(str(row.values["text"]))
        return result
    if not db.has_table("glossary_term"):
        return {}
    terms = {
        row.values["id"]: str(row.values["source_ja"])
        for row in db.rows("glossary_term")
        if (row.values["category"] == "card_name") == names
    }
    result = {}
    for row in db.rows("glossary_translation"):
        source = terms.get(row.values["term_id"])
        if source is not None and row.values["lang"] == "zh-Hant":
            result.setdefault(source, set()).add(str(row.values["text"]))
    return result


def _present(value: str, text: str) -> bool:
    pattern = LOOSE.get(value) or ".*".join(re.escape(p) for p in value.split("…"))
    return re.search(pattern, text) is not None


def issues(
    source: str, target: str, names: dict[str, set[str]], terms: dict[str, set[str]]
) -> tuple[str, ...]:
    """Check full card names, longest glossary matches, numbers and fixed answer openings."""
    errors = []
    for name in NAMES.findall(source):
        choices = names.get(name)
        if choices is None:
            errors.append("unknown_card_name")
        elif not any("『" + choice + "』" in target for choice in choices):
            errors.append("card_name")
    original = unicodedata.normalize("NFKC", NAMES.sub("", source))
    translated = unicodedata.normalize("NFKC", NAMES.sub("", target))
    if Counter(NUMBERS.findall(original)) != Counter(NUMBERS.findall(translated)):
        errors.append("numbers_or_q_number")
    if terms:
        pattern = re.compile(
            "|".join(
                re.escape(t) + {"場": "(?!合)", "エボルヴ": "(?!フォロワー)"}.get(t, "")
                for t in sorted(terms, key=len, reverse=True)
            )
        )
        errors.extend(
            "glossary"
            for term in set(pattern.findall(original))
            if not any(_present(value, translated) for value in terms[term])
        )
    for opening, expected in OPENINGS.items():
        if source.startswith(opening) and not target.startswith(expected):
            errors.append("answer_opening")
    return tuple(sorted(set(errors)))


def check(db: Database, entries: dict[Key, Entry]) -> tuple[str, ...]:
    """Return only hash and error codes; never print official wording in failure logs."""
    originals = sources(db)
    names, terms = vocabulary(db, names=True), vocabulary(db, names=False)
    errors = []
    for key, entry in entries.items():
        source_hash = entry.source_hash
        source = originals.get(key)
        if source is None:
            errors.append(source_hash + ": stale")
            continue
        errors.extend(
            source_hash + ": " + code
            for code in issues(source, entry.text, names, terms)
        )
    return tuple(errors)
