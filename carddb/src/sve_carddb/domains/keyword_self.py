"""Project region-owned self keywords without requiring effect DSL coverage."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build import Json
from sve_carddb.core.json import array

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.domains.mechanics import Inputs, Settings

ADAPTER = "printed-self-keywords-v1"
_JP_DECLARATION = re.compile(r"(?:【[^【】\n]+】\s*)+(?:[（(].*[）)])?")
_EN_DECLARATION = re.compile(r"(?:[A-Z][A-Za-z ]*\.\s*)+(?:\(.*\))?")


def _acquired(line: str, region: str, spellings: Mapping[str, str]) -> set[str]:
    if region == "jp":
        matches = re.finditer(
            r"これは(?:\{[^{}]+\}[+-][0-9]+(?:/\{[^{}]+\}[+-][0-9]+)?して、)?(?P<keywords>(?:【[^【】]+】)+)を持つ(?=。|$)",
            line,
        )
        names = {
            name
            for match in matches
            for name in re.findall(r"【([^【】]+)】", match["keywords"])
        }
    else:
        alternatives = "|".join(re.escape(value) for value in spellings.values())
        keywords = "(?:" + alternatives + ")"
        grant = re.compile(
            r"(?:\{[^{}]+\}[+-](?:[0-9]+|X)(?:/\{[^{}]+\}[+-](?:[0-9]+|X))?(?: and |, ))?(?P<keywords>"
            + keywords
            + r"(?:(?:, | and |, and )"
            + keywords
            + r")*)(?: until (?:the )?end of (?:the |your )?turn)?",
            re.IGNORECASE,
        )
        matches = re.finditer(
            r"(?:give this (?:follower|card|amulet)|this(?: follower|card|amulet)? (?:has|gains)) ([^.!?\n]+)",
            line,
            re.IGNORECASE,
        )
        names = {
            name
            for match in matches
            if (found := grant.fullmatch(match[1]))
            for name in re.findall(keywords, found["keywords"], re.IGNORECASE)
        }
    return {key for key, value in spellings.items() if value in names}


def classify(
    text: str, region: str, spellings: Mapping[str, str]
) -> tuple[set[str], set[str]]:
    """Return certain self declarations and keywords whose use still needs interpretation."""
    present: set[str] = set()
    uncertain: set[str] = set()
    quoted = False
    for raw in text.splitlines():
        line = raw.strip()
        in_quote = quoted or any(mark in line for mark in ("「", "」", '"', "“", "”"))
        quoted ^= bool(line.count('"') % 2)
        if "「" in line or "“" in line:
            quoted = True
        if "」" in line or "”" in line:
            quoted = False
        declaration = (
            _JP_DECLARATION.fullmatch(line)
            if region == "jp"
            else _EN_DECLARATION.fullmatch(line)
        )
        acquired = set[str]() if in_quote else _acquired(line, region, spellings)
        head = line.split("（")[0].split("(")[0]
        tokens = (
            re.findall(r"【([^【】]+)】", head)
            if region == "jp"
            else [part.strip() for part in head.split(".")]
        )
        for keyword, spelling in spellings.items():
            marker = (
                re.escape("【" + spelling + "】")
                if region == "jp"
                else r"\b" + re.escape(spelling) + r"\b"
            )
            if re.search(marker, line) is None:
                continue
            if keyword in acquired or (
                not in_quote and declaration and spelling in tokens
            ):
                present.add(keyword)
            else:
                uncertain.add(keyword)
    return present, uncertain - present


@dataclass
class _Fields:
    main: list[str]
    sections: list[str]
    missing: bool = False


def _fields(db: Database) -> dict[tuple[str, str], _Fields]:
    units = {
        str(row.values["id"]): str(row.values["text"]) for row in db.rows("text_unit")
    }
    revisions = {str(row.values["id"]): row.values for row in db.rows("face_revision")}
    current = {
        (str(row.values["face_id"]), str(row.values["region"])): str(
            row.values["revision_id"]
        )
        for row in db.rows("face_current")
    }
    faces: dict[str, list[str]] = {}
    for row in db.rows("face"):
        faces.setdefault(str(row.values["card_id"]), []).append(str(row.values["id"]))
    states = {
        str(row.values["id"]): str(row.values["identity_state"])
        for row in db.rows("card")
    }
    regions = {
        (str(row.values["card_id"]), str(row.values["region"]))
        for row in db.rows("printing")
        if states[str(row.values["card_id"])] != "retired"
    }
    sections: dict[str, list[str]] = {}
    for row in db.rows("face_text_section"):
        sections.setdefault(str(row.values["revision_id"]), []).append(
            units[str(row.values["text_unit_id"])]
        )
    result: dict[tuple[str, str], _Fields] = {}
    for cid, region in sorted(regions):
        fields = _Fields([], [], states[cid] != "confirmed" or not faces.get(cid))
        for face in faces.get(cid, []):
            revision_id = current.get((face, region))
            if revision_id is None:
                fields.missing = True
                continue
            revision = revisions[revision_id]
            fields.main.append(units[str(revision["effect_unit_id"])])
            fields.sections.extend(sections.get(revision_id, []))
        result[cid, region] = fields
    return result


def _relations(value: Value) -> set[str]:
    if not isinstance(value, Json):
        raise TypeError("Mechanic relations must be JSON")
    return {str(item) for item in array(value.value)}


def _project(
    db: Database, cid: str, scope: str, present: set[str], complete: set[str]
) -> set[str]:
    key: dict[str, Value] = {"card_id": cid, "scope": scope}
    projections = db.select(
        "mechanic_projection", db.columns("mechanic_projection"), where=key
    )
    positive = {
        str(row.values["keyword_id"])
        for row in projections
        if "has" in _relations(row.values["relations"])
    }
    for keyword in sorted(present):
        existing = next(
            (row for row in projections if row.values["keyword_id"] == keyword), None
        )
        relations = (
            {"has"}
            if existing is None
            else _relations(existing.values["relations"]) | {"has"}
        )
        if existing is None:
            db.insert(
                "mechanic_projection",
                key
                | {
                    "keyword_id": keyword,
                    "relations": Json(list[JsonValue](sorted(relations))),
                    "actions": Json([]),
                },
            )
        else:
            db.update(
                "mechanic_projection",
                key | {"keyword_id": keyword},
                {"relations": Json(list[JsonValue](sorted(relations)))},
            )
    if db.select("card_mechanic_coverage", ("card_id",), where=key):
        db.update(
            "card_mechanic_coverage",
            key,
            {"has_complete_keyword_ids": Json(list[JsonValue](sorted(complete)))},
        )
    else:
        db.insert(
            "card_mechanic_coverage",
            key
            | {
                "complete_all": False,
                "complete_mode": "include",
                "complete_keyword_ids": Json([]),
                "partial_mode": "include",
                "partial_keyword_ids": Json([]),
                "has_complete_keyword_ids": Json(list[JsonValue](sorted(complete))),
            },
        )
    return positive | present


def apply(db: Database, inputs: Inputs, settings: Settings) -> dict[str, JsonValue]:
    """Inspect every regional current face; missing faces and ambiguous uses stay unknown."""
    universe = {record.id for record in inputs.registry.records}
    fields = _fields(db)
    counts: dict[str, JsonValue] = {}
    for region in ("jp", "en"):
        spellings = {
            record.id: settings.classifier.terms[record.term_id].source_ja
            if region == "jp"
            else record.code.title()
            for record in inputs.registry.records
        }
        known_cards = 0
        total = 0
        for (cid, selected_region), selected in fields.items():
            if selected_region != region:
                continue
            total += 1
            present: set[str] = set()
            unknown = universe.copy() if selected.missing else set[str]()
            for main in selected.main:
                found, uncertain = classify(main, region, spellings)
                present.update(found)
                unknown.update(uncertain)
            # Supplemental token definitions never give their native keywords to the owner.
            for section in selected.sections:
                _, uncertain = classify(section, region, spellings)
                unknown.update(uncertain)
            complete = universe - unknown
            positive = _project(
                db,
                cid,
                "shared" if region == "jp" else "en_override",
                present,
                complete,
            )
            known_cards += int(universe <= positive | complete)
        counts[region] = {
            "total_cards": total,
            "known_cards": known_cards,
            "unknown_cards": total - known_cards,
        }
    return {"adapter": ADAPTER, "regions": counts}
