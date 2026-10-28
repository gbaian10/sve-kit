"""Read region-owned class and crossover mechanisms from printed current wording."""

import re
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build import Json

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database
    from sve_carddb.domains.keyword_self import CurrentFields

ADAPTER = "printed-mechanisms-v1"
CLASS = (
    "combo",
    "spellchain",
    "stack",
    "earth_rite",
    "overflow",
    "necrocharge",
    "sanguine",
)
COLLAB = ("serve", "lesson", "starting_amulet", "single_drive", "twin_drive")
CODES = CLASS + COLLAB
_JP = dict(
    zip(
        CODES,
        (
            "コンボ",
            "(?:スペルチェイン|SC)",
            "スタック",
            "土の秘術",
            "覚醒",
            "(?:ネクロチャージ|NC)",
            "真紅",
            "食事",
            "レッスン",
            "スタートアミュレット",
            "シングルドライブ",
            "ツインドライブ",
        ),
        strict=True,
    )
)
_EN = dict(
    zip(
        CODES,
        (
            "Combo",
            "(?:Spellchain|SC)",
            "Stack",
            "Earth Rite",
            "Overflow",
            "(?:Necrocharge|NC)",
            "Sanguine",
            "Serve",
            "Lesson",
            "Starting Amulet",
            "Single Drive",
            "Twin Drive",
        ),
        strict=True,
    )
)
_NUMBERED = {"combo", "spellchain", "necrocharge", "lesson"}
_DECLARATIONS = {"stack", "starting_amulet", "single_drive", "twin_drive"}
_PARENS = re.compile(r"[（(](?:[^（）()]|[（(][0-9]+[）)])*[）)]")


@dataclass
class Parsed:
    present: set[str] = field(default_factory=set)
    uncertain: set[str] = field(default_factory=set)
    thresholds: dict[str, set[int]] = field(default_factory=dict)
    balanced: bool = True


def _unquoted(text: str) -> tuple[str, bool]:
    result = []
    closing = ""
    for char in text:
        if closing:
            if char == closing:
                closing = ""
            result.append("\n" if char == "\n" else " ")
        elif char in {"「", "“", '"'}:
            closing = {"「": "」", "“": "”", '"': '"'}[char]
            result.append(" ")
        else:
            result.append(char)
    return "".join(result), not closing


def _native(
    line: str, code: str, name: str, marker: str, region: str, result: Parsed
) -> bool:
    found = False
    if code in _NUMBERED:
        numbered = (
            rf"【{name}_([0-9]+)】"
            if region == "jp"
            else rf"\b{name}\s*\(([0-9]+)\)\s*"
            + (r"[:,—–-]" if code == "lesson" else r"[:—–-]")
        )
        values = {int(match[1]) for match in re.finditer(numbered, line)}
        if values:
            result.thresholds.setdefault(code, set()).update(values)
            found = True
    elif code in _DECLARATIONS:
        declaration = (
            r"(?:【[^【】]+】\s*)+" if region == "jp" else r"(?:[A-Z][A-Za-z ]*\.\s*)+"
        )
        found = re.fullmatch(declaration, line) is not None
    elif code == "serve":
        found = (
            re.match(
                marker + r"\s*\{(?:コスト[0-9]+|icon_cost[0-9]+\|\[[^]]+\])\}\s*[:：]",
                line,
            )
            is not None
        )
    elif code == "earth_rite":
        found = bool(
            re.search(
                marker
                + (
                    r"(?!能力|を持つ|を使う|によって)"
                    if region == "jp"
                    else r"(?:\s*\([0-9]+\))?\s*[:—–-]"
                ),
                line,
            )
        )
    else:
        found = bool(
            re.search(
                marker
                + (
                    r"状態(?:なら|で|の|に|を)"
                    if region == "jp"
                    else r"\s+is\s+active(?:\s+for you)?"
                ),
                line,
            )
        )
    return found


def parse(text: str, region: str) -> Parsed:
    """Inspect native declarations, conditions and costs outside quotations and reminders."""
    text, balanced = _unquoted(text)
    previous = None
    while previous != text:
        previous, text = (
            text,
            _PARENS.sub(
                lambda match: (
                    match[0] if re.fullmatch(r"[（(][0-9]+[）)]", match[0]) else ""
                ),
                text,
            ),
        )
    result = Parsed(balanced=balanced)
    names = _JP if region == "jp" else _EN
    for raw in text.splitlines():
        line = raw.strip()
        for code, name in names.items():
            marker = rf"【{name}(?:_[0-9]+)?】" if region == "jp" else rf"\b{name}\b"
            if code == "serve":
                marker = r"\{食事\}" if region == "jp" else r"\{icon_carrot\|\[feed\]\}"
            if not re.search(marker, line):
                continue
            found = _native(line, code, name, marker, region, result)
            (result.present if found else result.uncertain).add(code)
    result.uncertain -= result.present
    return result


def _section_owner(text: str, region: str) -> str:
    line = text.lstrip()
    if not line or line.startswith(("（", "(")):
        return "reminder"
    if region == "jp":
        if re.match(r"『[^』]+』\{", line):
            return "token"
        if re.match(
            r"(?:\{(?:進化|超進化|ファンファーレ|ラストワード|起動|食事)\}|【|これは|『[^』]+』[0-9]+(?:体|つ|枚)を)",
            line,
        ):
            return "self"
    elif re.match(
        r"(?:Summon |\{icon_(?:evolve|superevolve|fanfare|lastword|act|carrot)\||(?:Ward|Storm|Rush|Bane|Drain|Stack|Starting Amulet|Single Drive|Twin Drive)\.)",
        line,
    ):
        return "self"
    return "unknown"


def apply(
    db: Database, fields: Mapping[tuple[str, str], CurrentFields]
) -> dict[str, JsonValue]:
    """Keep uncertain owners and incomplete regional faces out of negative coverage."""
    counts: dict[str, JsonValue] = {}
    for region in ("jp", "en"):
        by_code = dict.fromkeys(CODES, 0)
        unknown_cards = 0
        for (cid, selected_region), current in fields.items():
            if selected_region != region:
                continue
            present: set[str] = set()
            uncertain = set(CODES) if current.missing else set[str]()
            thresholds: dict[str, set[int]] = {}
            for text, owner in [(text, "self") for text in current.main] + [
                (text, _section_owner(text, region)) for text in current.sections
            ]:
                if owner in {"token", "reminder"}:
                    continue
                parsed = parse(text, region)
                if not parsed.balanced:
                    uncertain.update(CODES)
                if owner == "unknown":
                    uncertain.update(parsed.present | parsed.uncertain)
                    continue
                present.update(parsed.present)
                uncertain.update(parsed.uncertain)
                for code, values in parsed.thresholds.items():
                    thresholds.setdefault(code, set()).update(values)
            checked = set(CODES) - uncertain
            unknown_cards += bool(uncertain - present)
            for code in present:
                by_code[code] += 1
            db.update(
                "card_mechanic_coverage",
                {
                    "card_id": cid,
                    "scope": "shared" if region == "jp" else "en_override",
                },
                {
                    "printed_mechanisms": Json(
                        [
                            {
                                "code": code,
                                "thresholds": list[JsonValue](
                                    sorted(thresholds.get(code, set()))
                                ),
                            }
                            for code in sorted(present)
                        ]
                    ),
                    "checked_mechanisms": Json(list[JsonValue](sorted(checked))),
                },
            )
        counts[region] = {
            "by_code": dict[str, JsonValue](by_code),
            "unknown_cards": unknown_cards,
        }
    return {"adapter": ADAPTER, "regions": counts}
