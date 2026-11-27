"""Read native ability headers from current regional card wording."""

import re
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build import Json
from sve_carddb.core.wording import quote_after

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database
    from sve_carddb.domains.keyword_self import CurrentFields

ADAPTER = "ability-heads-v1"
_JP = {
    "クイック": "quick",
    "ファンファーレ": "fanfare",
    "ラストワード": "last_words",
    "進化時": "on_evolve",
    "攻撃時": "on_attack",
    "アドバンス起動": "advanced_act",
    "超進化時": "on_super_evolve",
    "起動": "act",
    "融合": "fusion",
}
_EN_ICONS = {
    "quick": "quick",
    "q": "quick",
    "fanfare": "fanfare",
    "lastword": "last_words",
    "lastwords": "last_words",
    "act": "act",
    "adv": "advanced_act",
}
_JP_HEAD = re.compile(r"^(?:\{([^{}]+)\}|【([^【】]+)】|《([^《》]+)》)\s*")
_EN_ICON = re.compile(r"^\{icon_([^{}|]+)\|\[[^\]]+\]\}\s*")
_EN_HEADS = {
    "On Super-Evolve": "on_super_evolve",
    "On Evolve": "on_evolve",
    "On Attack": "on_attack",
    "Strike": "on_attack",
    "Fanfare": "fanfare",
    "Last Words": "last_words",
    "Quick": "quick",
    "Advanced Activate": "advanced_act",
    "Activate": "act",
    "Fusion": "fusion",
}
_EN_NAMES = "|".join(
    re.escape(name) for name in sorted(_EN_HEADS, key=len, reverse=True)
)
_EN_HEAD = re.compile(
    rf"(?P<names>(?:{_EN_NAMES})(?:/(?:{_EN_NAMES}))*)(?:\s*[-–—:.,]|\s*\()"
)
_EN_EVOLUTION = re.compile(r"^\{icon_evolve\|\[evolve\]\}")


def parse(text: str, region: str) -> tuple[set[str], bool]:
    """Inspect headers before body text; quoted abilities and token supplements stay with their owner."""
    line_heads, balanced = _heads(text, region)
    return {head for heads in line_heads.values() for head in heads}, balanced


def _heads(text: str, region: str) -> tuple[dict[int, set[str]], bool]:
    result: dict[int, set[str]] = {}
    quoted = False
    for number, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        was_quoted = quoted
        quoted = quote_after(quoted, line)
        if was_quoted:
            continue
        rest = line
        line_types: set[str] = set()
        while match := (
            _JP_HEAD.match(rest) if region == "jp" else _EN_ICON.match(rest)
        ):
            name = next(group for group in match.groups() if group is not None)
            code = (_JP if region == "jp" else _EN_ICONS).get(name)
            if code is not None:
                line_types.add(code)
            rest = rest[match.end() :]
        if region == "en":
            if head := _EN_HEAD.match(rest):
                line_types.update(_EN_HEADS[name] for name in head["names"].split("/"))
            if "act" in line_types and re.match(r"Fuse\s+[^:]+:", rest):
                line_types.add("fusion")
        result[number] = line_types
    return result, not quoted


def english_author_headers(text: str) -> dict[int, set[str]]:
    """Include the Evolve action marker alongside the English timing headers."""
    lines, _ = _heads(text, "en")
    for number, raw in enumerate(text.splitlines(), 1):
        if number in lines and _EN_EVOLUTION.match(raw.strip()):
            lines[number].add("evolve")
    return lines


def apply(
    db: Database, fields: Mapping[tuple[str, str], CurrentFields]
) -> dict[str, JsonValue]:
    """Store region-owned headers alongside the independent self keyword coverage."""
    known: dict[str, int] = {"jp": 0, "en": 0}
    total: dict[str, int] = {"jp": 0, "en": 0}
    for (cid, region), current in fields.items():
        selected: set[str] = set()
        complete = not current.missing
        for main in current.main:
            found, balanced = parse(main, region)
            selected.update(found)
            complete &= balanced
        db.update(
            "card_mechanic_coverage",
            {"card_id": cid, "scope": "shared" if region == "jp" else "en_override"},
            {
                "ability_types": Json(list[JsonValue](sorted(selected))),
                "ability_types_complete": complete,
            },
        )
        total[region] += 1
        known[region] += int(complete)
    return {
        "adapter": ADAPTER,
        "regions": {
            region: {
                "total_cards": total[region],
                "known_cards": known[region],
                "unknown_cards": total[region] - known[region],
            }
            for region in total
        },
    }
