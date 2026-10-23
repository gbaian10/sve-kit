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
FRAGMENTS = re.compile(r"カード名に『[^』]+』(?:か『[^』]+』)*を含(?:む|まない)")
EVOLVE = re.compile(r"[（(]EVOLVE[）)]$")
NUMBERS = re.compile(r"Q[0-9]+|[0-9]+")
# Style puts the object between the verb and the destination, and glossary values elide objects with "…".
LOOSE = {
    "召喚到戰場上": "召喚.*到(?:自己|敵方)?(?:的)?戰場上",
    "進入戰場時": "進入(?:自己|敵方)?(?:的)?戰場時",
    "卡片名稱": "(?:卡片名稱|日文卡名)",
}
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
        for source, choices in _glossary(db, names=True).items():
            result.setdefault(source, choices)
        return result
    return _glossary(db, names=False)


def _glossary(db: Database, *, names: bool) -> dict[str, set[str]]:
    if not db.has_table("glossary_term"):
        return {}
    terms = {
        row.values["id"]: str(row.values["source_ja"])
        for row in db.rows("glossary_term")
        if (row.values["category"] == "card_name") == names
    }
    result: dict[str, set[str]] = {}
    for row in db.rows("glossary_translation"):
        source = terms.get(row.values["term_id"])
        if source is not None and row.values["lang"] == "zh-Hant":
            result.setdefault(source, set()).add(str(row.values["text"]))
    return result


def _present(value: str, text: str) -> bool:
    pattern = LOOSE.get(value) or ".*".join(re.escape(p) for p in value.split("…"))
    return re.search(pattern, text) is not None


def _normalized(text: str) -> str:
    # NFKC must not join a numbered list marker to the first number in its item.
    separated = re.sub(r"[①-⑳]", lambda match: " " + match[0] + " ", text)
    return unicodedata.normalize("NFKC", NAMES.sub("", separated))


def issues(
    source: str, target: str, names: dict[str, set[str]], terms: dict[str, set[str]]
) -> tuple[str, ...]:
    """Check full card names, longest glossary matches, numbers and fixed answer openings."""
    errors = []
    fragments = [match.span() for match in FRAGMENTS.finditer(source)]
    for match in NAMES.finditer(source):
        name = match[1]
        if any(start <= match.start() < end for start, end in fragments):
            if "『" + name + "』" not in target:
                errors.append("card_name")
            continue
        base = EVOLVE.sub("", name)
        choices = names.get(base)
        if choices is None:
            errors.append("unknown_card_name")
        elif not any(
            "『" + choice + ("（EVOLVE）" if base != name else "") + "』" in target
            for choice in choices
        ):
            errors.append("card_name")
    original = _normalized(source)
    translated = _normalized(target)
    if Counter(NUMBERS.findall(original)) != Counter(NUMBERS.findall(translated)):
        errors.append("numbers_or_q_number")
    if terms:
        pattern = re.compile(
            "|".join(
                {"場": "(?<!置き)"}.get(t, "")
                + re.escape(t)
                + {
                    "場": "(?!合)",
                    "エボルヴ": "(?!フォロワー)",
                    "プレイ": "(?!ヤー)",
                    "デッキの上": "(?!半分)",
                }.get(t, "")
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
