# /// script
# requires-python = ">=3.12"
# dependencies = ["pdfplumber==0.11.11"]
# ///
"""Convert a local official comprehensive-rules PDF to the web rules data format."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from dataclasses import dataclass
from datetime import date, datetime
from itertools import pairwise
from operator import itemgetter
from pathlib import Path
from typing import TYPE_CHECKING, Any

import pdfplumber
from pdfplumber.utils import cluster_objects, extract_text

if TYPE_CHECKING:
    from collections.abc import Iterator

    from pdfplumber.page import Page


type PdfObject = dict[str, Any]
type JsonObject = dict[str, Any]

CLAUSE = re.compile(r"^(\d+(?:\.\d+)*)\. +(.+)$")
APPENDIX = re.compile(r"^(?:付録|Appendix) ?([ABC])[：:] ?(.+)$")
FORMAT = re.compile(r"^(B-[12])[：:] ?(.+)$")
APPENDIX_CLAUSE = re.compile(r"^(\d+(?:\.\d+)*)\.? +(.+)$")
HISTORY = re.compile(r"^\d{4}年\d{1,2}月\d{1,2}日 +ver\.? +\d+\.\d+\.\d+$")
EXAMPLE = re.compile(r"^(?:[-–] *)?(例：|Example:) *")
JP = r"[^\x00-\x7f]"
RED_COMPONENT_MINIMUM = 0.8
MIN_PRINTED_GLYPH_SIZE = 2
DIAGRAM_LABEL_GAP = 25
CARD_IMAGE_WIDTH = 90
CLASS_ICON_WIDTH = 30
HEADING_DEPTH = 2
CLASSES = {
    "jp": [
        "ニュートラル",
        "エルフ",
        "ロイヤル",
        "ウィッチ",
        "ドラゴン",
        "ナイトメア",
        "ビショップ",
    ],
    "en": [
        "neutral",
        "forestcraft",
        "swordcraft",
        "runecraft",
        "dragoncraft",
        "abysscraft",
        "havencraft",
    ],
}
CLASS_ICONS = ["neutral", "elf", "royal", "witch", "dragon", "nightmare", "bishop"]
# The same figure can have several image encodings across editions or sizes.
ICONS = {
    "jp": {
        "b27ebcbb0bd6": "攻撃力",
        "ca4dcff117d2": "体力",
        "a03710122f83": "PP",
        "c3ffb748ffa7": "コストX",
        "f258eb570260": "起動",
        "924657162beb": "アクト",
        "66f36fcca4ae": "ファンファーレ",
        "dc01c09f8e56": "ラストワード",
        "6dd695f26ed5": "進化",
        "1cd43135884b": "クイック",
        "977c3b1a4c55": "Q",
        "6917788d0566": "アドバンス起動",
        "4986a3c1846b": "食事",
        "ba68d11fee89": "クリティカルトリガー",
        "593de02de14e": "ドロートリガー",
        "23e6bcfbce93": "スタンドトリガー",
        "18285eff9b70": "ヒールトリガー",
        "b2bb51c3e43e": "憑依",
        "3e5d500fcba5": "UB",
    },
    "en": {
        "bc4466ea52e8": "icon_power|[attack]",
        "1369bed68229": "icon_hp|[defense]",
        "a03710122f83": "PP",
        "c3ffb748ffa7": "COST X|[costX]",
        "af4482a273ca": "icon_stand|[engage]",
        "f258eb570260": "icon_stand|[engage]",
        "1961814d1751": "icon_act|[act]",
        "892c17340f45": "icon_act|[act]",
        "e78f7f2e55fd": "icon_fanfare|[fanfare]",
        "01c4141e1052": "icon_lastword|[lastwords]",
        "1d0559fbe06c": "icon_evolve|[evolve]",
        "b543ca067dee": "icon_quick|[quick]",
        "d1cc63f9fa0c": "icon_q|[q]",
        "6917788d0566": "icon_adv|[adv]",
        "962255fe3122": "icon_carrot|[feed]",
        "d7d1c8011181": "icon_carrot|[feed]",
        "405431c6ab53": "Critical Trigger",
        "a23228089960": "Draw Trigger",
        "4d93df514262": "Stand Trigger",
        "1d14a3a0d449": "Heal Trigger",
        "4cdf762b46b7": "icon_ride|[ride]",
        "3e5d500fcba5": "icon_UB|[UB]",
    },
}
PRINTED_MARKERS = {
    "FANFARE": "icon_fanfare|[fanfare]",
    "LAST WORDS": "icon_lastword|[lastwords]",
    "ATK": "icon_power|[attack]",
    "DEF": "icon_hp|[defense]",
    "ACT": "icon_act|[act]",
    "ENGAGE": "icon_stand|[engage]",
    "QUICK": "icon_quick|[quick]",
    "UB": "icon_UB|[UB]",
    **{
        name.upper(): f"icon_{icon}|[{name}]"
        for icon, name in zip(CLASS_ICONS, CLASSES["en"], strict=True)
    },
}
TRIGGER_ICONS = {
    "jp": (
        "クリティカルトリガー",
        "ドロートリガー",
        "スタンドトリガー",
        "ヒールトリガー",
    ),
    "en": ("Critical Trigger", "Draw Trigger", "Stand Trigger", "Heal Trigger"),
}


@dataclass(frozen=True)
class _Line:
    chars: list[PdfObject]

    @property
    def top(self) -> float:
        """The upper edge of the line."""
        return min(char["top"] for char in self.chars)

    @property
    def bottom(self) -> float:
        """The lower edge of the line."""
        return max(char["bottom"] for char in self.chars)

    @property
    def text(self) -> str:
        """The text read from left to right."""
        chars = sorted(self.chars, key=itemgetter("x0"))
        return extract_text(chars, x_tolerance=1.5, y_tolerance=4).strip()

    @property
    def red(self) -> bool:
        """Whether any visible character uses the update color."""
        return any(
            isinstance(rgb := char["non_stroking_color"], tuple)
            and rgb[-2:] == (0, 0)
            and rgb[0] >= RED_COMPONENT_MINIMUM
            for char in self.chars
            if char["text"].strip()
        )


def _lines(chars: list[PdfObject]) -> list[_Line]:
    visible = [
        char
        for char in chars
        if char["size"] > MIN_PRINTED_GLYPH_SIZE
        and char["non_stroking_color"] not in {(1,), (1, 1, 1)}
    ]
    return [
        _Line(group)
        for group in cluster_objects(visible, "doctop", tolerance=2)
        if any(c["text"].strip() for c in group)
    ]


def _normalize(text: str, region: str) -> str:
    text = text.strip()
    if region == "en":
        icon = r"\{[^}]+\}"
        text = re.sub(rf'([("])( +)({icon})', r"\1\3", text)
        return re.sub(rf"({icon}) +([).,:])", r"\1\2", text)
    text = re.sub(rf"(?<={JP}) +(?=[A-Za-z0-9])|(?<=[A-Za-z0-9]) +(?={JP})", "", text)
    return re.sub(r"(?<=}) +| +(?={)", "", text)


def _join_lines(parts: list[str], region: str) -> str:
    if region == "jp":
        return "".join(parts)
    text = ""
    for part in parts:
        if text and part and not text.endswith(("-", "–", "—")):
            text += " "
        text += part
    # _Line wraps can separate the dash from the bracket in rules notation.
    text = re.sub(r"(?<=\) )-(?=\[)", "- ", text)
    return _normalize(text, region)


def _metadata(lines_on_cover: list[_Line], region: str) -> tuple[str, str]:
    title, updated = (line.text for line in lines_on_cover[:2])
    version = re.search(r"\bver\. *(\d+\.\d+\.\d+)\s*$", title, re.IGNORECASE)
    if not version:
        raise ValueError("Version missing from the PDF cover")
    if region == "jp":
        updated = _normalize(updated, region)
        match = re.fullmatch(r"最終更新日：(\d+)年(\d+)月(\d+)日", updated)
        if not match:
            raise ValueError("Update date missing from the PDF cover")
        last_updated = date(*map(int, match.groups()))
    else:
        # The cover gives a calendar date without a time or time zone.
        last_updated = datetime.strptime(
            updated + " +0000", "Last Updated: %B %d, %Y %z"
        ).date()
    return version[1], last_updated.isoformat()


def _icon_marker(image: PdfObject, region: str) -> str:
    digest = hashlib.sha256(image["stream"].get_rawdata()).hexdigest()[:12]
    if digest not in ICONS[region]:
        raise ValueError(f"Unknown {region} inline icon {digest}")
    return ICONS[region][digest]


def _add_icons(body: list[_Line], images: list[PdfObject], region: str) -> None:
    for image in images:
        middle = (image["top"] + image["bottom"]) / 2
        nearest = min(body, key=lambda line: abs((line.top + line.bottom) / 2 - middle))
        char = nearest.chars[0]
        nearest.chars.append(
            {
                **char,
                "x0": image["x0"],
                "x1": image["x1"],
                "text": "{" + _icon_marker(image, region) + "}",
                "non_stroking_color": (0,),
            }
        )


def _split_labels(line: _Line, region: str) -> list[str]:
    labels = [[]]
    previous = None
    for char in sorted(line.chars, key=itemgetter("x0")):
        if previous is not None and char["x0"] - previous > DIAGRAM_LABEL_GAP:
            labels.append([])
        labels[-1].append(char)
        previous = char["x1"]
    return [
        _normalize(_Line(label).text, region) for label in labels if _Line(label).text
    ]


def _figure_caption(chars: list[PdfObject], legend_start: float, region: str) -> str:
    legend = [
        _normalize(line.text, region)
        for line in _lines([c for c in chars if c["x0"] > legend_start])
    ]
    labels = []
    for line in _lines([c for c in chars if c["x0"] <= legend_start]):
        labels.extend(_split_labels(line, region))
    if region == "jp":
        legend = [re.sub(r"^([①-⑳]) +", r"\1", item) for item in legend]
    return ("　" if region == "jp" else " / ").join(labels[:2] + legend + labels[2:])


def _text_events(page: Page, region: str) -> Iterator[_Line | JsonObject]:
    """Two columns resume below the full-width card diagram."""
    large = [image for image in page.images if image["width"] > CARD_IMAGE_WIDTH]
    illustrations = [
        image
        for image in page.images
        if CLASS_ICON_WIDTH < image["width"] <= CARD_IMAGE_WIDTH
    ]
    table = None
    if len(illustrations) == len(CLASSES[region]):
        table = max(page.find_tables(), key=lambda item: len(item.cells))
    x_middle = page.width / 2

    def columns(chars: list[PdfObject], images: list[PdfObject]) -> Iterator[_Line]:
        for left in (True, False):
            body = _lines([char for char in chars if (char["x0"] < x_middle) == left])
            _add_icons(
                body,
                [image for image in images if (image["x0"] < x_middle) == left],
                region,
            )
            yield from body

    chars = page.chars
    icons = [
        image
        for image in page.images
        if image not in large and not (table and image in illustrations)
    ]
    if table:
        x0, top, x1, bottom = table.bbox
        legend = [
            char
            for char in chars
            if x0 <= char["x0"] < x1 and top <= char["top"] < bottom
        ]
        names = " ".join(line.text for line in _lines(legend)).lower().split()
        if names != CLASSES[region]:
            raise ValueError(f"Unexpected class legend on page {page.page_number}")
        label_ids = {id(char) for char in legend}
        chars = [char for char in chars if id(char) not in label_ids]
    if not large:
        yield from columns(chars, icons)
        return
    # Use the image band to separate the caption and both columns of text.
    top = min(image["top"] for image in large) - 7
    bottom = max(image["bottom"] for image in large) + 12
    before = list(columns([c for c in chars if c["top"] < top], []))
    heading = next(line for line in before if line.text.startswith("2. "))
    yield from (line for line in before if line is not heading)
    yield heading
    figure_chars = [c for c in chars if top <= c["top"] < bottom]
    legend_start = max(image["x1"] for image in large)
    yield {
        "page": page.page_number,
        "caption": _figure_caption(figure_chars, legend_start, region),
    }
    yield from columns([c for c in chars if c["top"] >= bottom], icons)


class _Rules:
    def __init__(self, region: str) -> None:
        self.region = region
        self.context = "main"
        self.files: dict[str, list[JsonObject]] = {}
        self.chapter_numbers: dict[str, str] = {}
        self.node: JsonObject = {}
        self.parts: list[list[str]] = []
        self.examples: list[list[str]] = []
        self.wrapped_title = False

    def finish(self) -> None:
        """Complete the text and formatting of the current node."""
        if not self.node:
            return
        self.node["text"] = "\n".join(
            _join_lines(parts, self.region) for parts in self.parts
        )
        if self.examples:
            self.node["examples"] = [
                _join_lines(parts, self.region) for parts in self.examples
            ]
        if self.wrapped_title and self.parts:
            self.node["title"] = _join_lines(
                [self.node["title"], self.node["text"]], self.region
            )
            self.node["text"] = ""
        if self.node["key"] == "2.2.2":
            names = CLASSES[self.region]
            if self.region == "jp":
                legends = [f"{{{name}}}{name}" for name in names]
            else:
                legends = [
                    f"{{icon_{icon}|[{name}]}} {name.title()}"
                    for icon, name in zip(CLASS_ICONS, names, strict=True)
                ]
            self.node["text"] += "\n" + "\n".join(legends)
        if self.node.pop("changed", False):
            self.node["changed"] = True

    def _start(
        self,
        key: str,
        kind: str,
        depth: int,
        *,
        number: str | None = None,
        title: str | None = None,
        file: str | None = None,
    ) -> None:
        self.finish()
        self.parts = []
        self.examples = []
        self.wrapped_title = False
        if file:
            self.files[file] = []
        self.node = {"key": key, "context": self.context}
        if number is not None:
            self.node["number"] = number
        self.node.update(kind=kind, depth=depth)
        if title is not None:
            self.node["title"] = title
        self.node["text"] = ""
        self.files[next(reversed(self.files))].append(self.node)

    def consume(self, line: _Line | JsonObject) -> None:
        """Append a body line or card diagram to the rules."""
        if isinstance(line, dict):
            self.node["figure"] = line
            return
        text = line.text
        if self._numbered(text):
            if line.red:
                self.node["changed"] = True
            return
        if match := APPENDIX.fullmatch(text):
            self._appendix(*match.groups())
        elif match := FORMAT.fullmatch(text):
            self.context = match[1]
            self._start(
                match[1],
                "section",
                2,
                title=f"{match[1]}: {match[2]}"
                if self.region == "en"
                else f"{match[1]}：{match[2]}",
            )
        elif self.context == "C" and HISTORY.fullmatch(
            re.sub(r"日 *ver", "日 ver", _normalize(text, self.region))
        ):
            history = _normalize(text, self.region)
            history = re.sub(r"日 *ver\.? *", "日 ver ", history)
            count = len(self.files["appendix-c.json"])
            self._start(f"C:{count}", "block", 2, title=history)
        elif match := EXAMPLE.match(text):
            separator = (
                " "
                if self.region == "en" or text[len(match[1]) :].startswith(" ")
                else ""
            )
            self.examples.append(
                [match[1] + separator + _normalize(text[match.end() :], self.region)]
            )
        elif (
            self.context == "C" and text.startswith("\uf077 （") and text.endswith("）")
        ):
            self.parts.append([_normalize(text[1:], self.region)])
        elif text.startswith(("*", "＊", "\uf077", "•")) or (
            self.node["key"] == "5.30.2" and text.startswith("- ")
        ):
            self.parts.append(["* " + _normalize(text[1:], self.region)])
        elif self.node["key"] == "14.4.1.1" and any(
            text.endswith(name) for name in TRIGGER_ICONS[self.region]
        ):
            self.parts.append([_normalize(text, self.region)])
        elif self.examples:
            self.examples[-1].append(_normalize(text, self.region))
        elif self.parts:
            self.parts[-1].append(_normalize(text, self.region))
        else:
            self.parts.append([_normalize(text, self.region)])
        if line.red:
            self.node["changed"] = True

    def _numbered(self, text: str) -> bool:
        clause = APPENDIX_CLAUSE if self.context in {"B-1", "B-2"} else CLAUSE
        match = clause.fullmatch(text)
        if match and (
            clause is CLAUSE
            or "." in match[1]
            or _normalize(match[2], self.region) == self.chapter_numbers.get(match[1])
        ):
            number, body = match.groups()
            depth = number.count(".") + 1
            main = self.context == "main"
            kind = (
                "rule"
                if depth > HEADING_DEPTH
                else "chapter"
                if main and depth == 1
                else "section"
            )
            self._start(
                number if main else f"{self.context}:{number}",
                kind,
                depth if main else depth + 2,
                number=number,
                title=_normalize(body, self.region) if depth <= HEADING_DEPTH else None,
                file=f"ch{int(number):02}.json" if main and depth == 1 else None,
            )
            if main and depth == 1:
                self.chapter_numbers[number] = self.node["title"]
            if depth > HEADING_DEPTH:
                self.parts.append([_normalize(body, self.region)])
            else:
                self.wrapped_title = self.region == "en" and main
            return True
        return False

    def _appendix(self, key: str, title: str) -> None:
        self.context = key
        heading = (
            f"付録{key}：{_normalize(title, self.region)}"
            if self.region == "jp"
            else f"Appendix {key}: {title}"
        )
        self._start(
            key, "chapter", 1, title=heading, file=f"appendix-{key.lower()}.json"
        )

    def tables(self, page: Page) -> None:
        """Append the token rows on the page."""
        tables = page.find_tables()
        if len(tables) != 1:
            raise ValueError(
                f"Expected one token table on page {page.page_number}, got {len(tables)}"
            )
        table = tables[0]
        headings = _lines([char for char in page.chars if char["top"] < table.bbox[1]])
        self._table_headings(headings, page.page_number)
        # Colored class cells can introduce extra horizontal edges. The card name column
        # spans each complete row, even when the other cells are split by those edges.
        columns = [column.bbox[0] for column in table.columns] + [table.bbox[2]]
        bounds = [
            (row.cells[0][1], row.cells[0][3]) for row in table.rows if row.cells[0]
        ]
        if self.region == "jp":
            bounds.insert(0, (headings[-1].top - 1, table.bbox[1]))
        for index, (top, bottom) in enumerate(bounds):
            cells = []
            for x0, x1 in pairwise(columns):
                chars = [
                    c
                    for c in page.chars
                    if x0 <= (c["x0"] + c["x1"]) / 2 < x1
                    and top <= (c["top"] + c["bottom"]) / 2 < bottom
                ]
                cell_lines = _lines(chars)
                if any(line.red for line in cell_lines):
                    self.node["changed"] = True
                parts = [_normalize(line.text, self.region) for line in cell_lines]
                if (
                    cells
                    and x1 == columns[-1]
                    and cells[0] not in {"カード名", "Card Name"}
                ):
                    cells.append(_token_text(parts, cells[0], self.region))
                else:
                    cells.append(_join_lines(parts, self.region))
            if cells[0] in {"カード名", "Card Name"}:
                data = self.node.setdefault("table", {"header": cells, "rows": []})
                if index != 0 or data["header"] != cells:
                    raise ValueError(
                        f"Unexpected token table header on page {page.page_number}"
                    )
            elif not cells[0] and cells[-1] and not any(cells[:-1]):
                previous = self.node["table"]["rows"][-1]
                separator = "\n" if previous[-1].endswith("----------") else " "
                previous[-1] += separator + cells[-1]
            elif cells[0]:
                self.node["table"]["rows"].append(cells)
            else:
                raise ValueError(
                    f"Token table row has no name on page {page.page_number}"
                )

    def _table_headings(self, headings: list[_Line], page_number: int) -> None:
        for line in headings:
            if match := APPENDIX.fullmatch(line.text):
                self._appendix(*match.groups())
            elif line.text.startswith("●"):
                title = _normalize(line.text.removeprefix("●"), self.region)
                if title.endswith("（続き）"):
                    if self.node["title"] != title.removesuffix("（続き）").rstrip():
                        raise ValueError(
                            f"Unexpected token table continuation on page {page_number}"
                        )
                    continue
                self._start(
                    f"A:{len(self.files['appendix-a.json'])}", "block", 2, title=title
                )
                if line.red:
                    self.node["changed"] = True


# These paragraph starts are indistinguishable from soft wraps in the PDF.
TOKEN_PARAGRAPHS = {
    "jp": {
        "インモラルディザイア": ("相手のリーダー",),
        "奏絶の独唱": ("相手の場のフォロワー",),
        "ユニオン・マジック": ("相手の場のフォロワー",),
        "黒龍の呪い": ("これは【覚醒】", "自分の場の『竜槍の戦士"),
        "テラーナイト": ("このターン、自分のリーダー",),
        "プリンセスソード": ("これを装備しているフォロワーが受ける",),
    },
    "en": {
        "Ghost": ("At the start",),
        "Gargantuan Ghost": ("At the start",),
        "Lloyd": ("While this is on the field",),
        "Victoria": ("While this is on the field",),
        "Devoted Dragon": ("While an Aiela",),
        "Anne's Summoning": ("At the start",),
        "Bansai Suzuki, Clone Technique": ("At the start",),
        "Ephemeral Moon": ("At the start",),
        "Val, Trusty Getaway Car": ("Storm.", "Strike -"),
        "Magitrain": ("Rush.", "Strike -"),
        "Curse of the Black Dragon": ("At the start",),
        "Princess Sword": ("If the equipped follower would take damage",),
        "Glorious Feather": ("If the equipped follower would deal damage",),
    },
}


def _token_text(parts: list[str], name: str, region: str) -> str:
    paragraphs = []
    for part in parts:
        paragraph_start = part.startswith(TOKEN_PARAGRAPHS[region].get(name, ()))
        if region == "jp":
            boundary = bool(
                paragraphs
                and (
                    re.match(
                        r"(?:【(?![2-9]】)|《(?:ファンファーレ|ラストワード|起動|クイック)》)",
                        part,
                    )
                    or re.fullmatch(r"(?:【[^】]+】)+|《クイック》", paragraphs[-1][-1])
                )
            )
        else:
            boundary = bool(
                paragraphs
                and (
                    part.startswith("----------")
                    or paragraphs[-1][-1] == "----------"
                    or re.match(r"\{(?:FANFARE|LAST WORDS|ACT|QUICK)\}", part)
                    or paragraphs[-1][-1] == "{QUICK}"
                )
            )
        if name in {
            "魔導四輪車・V",
            "新約・白の章",
            "新約・黒の章",
        } and part.startswith("《起動》"):
            boundary = False
        if boundary or paragraph_start or not paragraphs:
            paragraphs.append([])
        paragraphs[-1].append(part)
    text = "\n".join(_join_lines(paragraph, region) for paragraph in paragraphs)
    if region == "en":
        text = re.sub(r"\{([^}]+)\}", lambda m: "{" + _marker_name(m[1]) + "}", text)
    return text


def _marker_name(name: str) -> str:
    match = re.fullmatch(r"COST ([0-9]+)", name)
    if match:
        cost = f"cost{int(match[1]):02}"
        return f"icon_{cost}|[{cost}]"
    return PRINTED_MARKERS.get(name, name)


def parse(
    pdf: Path, region: str, page_url: str, source_url: str
) -> dict[str, JsonObject]:
    """Return the version index and chapter/appendix files from a local PDF."""
    rules = _Rules(region)
    with pdf.open("rb") as source, pdfplumber.open(source) as document:
        cover = document.pages[0]
        version, last_updated = _metadata(
            _lines([c for c in cover.chars if c["x0"] < cover.width / 2]), region
        )
        for page in document.pages:
            heads = _lines(page.chars)
            token_page = any(
                re.match(r"^(?:付録 ?A|Appendix A)[：:] ?.+$", line.text)
                and not re.search(r"\.{3}", line.text)
                for line in heads
            )
            format_page = any(
                re.match(r"^(?:付録 ?B|Appendix B)[：:] ?.+$", line.text)
                for line in heads
            )
            if token_page or (rules.context == "A" and not format_page):
                rules.tables(page)
            else:
                events = list(_text_events(page, region))
                if page is cover:
                    body_start = next(
                        i
                        for i, line in enumerate(events)
                        if isinstance(line, _Line)
                        and line.text.startswith("1. ")
                        and not re.search(r"\.{3}", line.text)
                    )
                    events = events[body_start:]
                for event in events:
                    rules.consume(event)
            page.close()
        rules.finish()
        pages = len(document.pages)
        source.seek(0)
        raw_sha256 = hashlib.file_digest(source, "sha256").hexdigest()
    index: JsonObject = {
        "format": 1,
        "kind": "comprehensive-rules-index",
        "version": version,
        "region": region,
        "last_updated": last_updated,
        "page_url": page_url,
        "source_url": source_url,
        "raw_sha256": raw_sha256,
        "pages": pages,
        "contents": [],
    }
    output = {"index.json": index}
    for file, nodes in rules.files.items():
        first = nodes[0]
        title = first["title"]
        if "number" in first:
            title = f"{first['number']}. {title}"
        index["contents"].append({"file": file, "key": first["key"], "title": title})
        output[file] = {
            "format": 1,
            "kind": "comprehensive-rules",
            "version": version,
            "nodes": nodes,
        }
    return output


def _main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pdf", type=Path)
    parser.add_argument("output", type=Path, help="new version directory")
    parser.add_argument("--region", required=True, choices=("jp", "en"))
    parser.add_argument("--page-url", required=True, help="official rules page URL")
    parser.add_argument("--source-url", required=True, help="official PDF URL")
    args = parser.parse_args()
    try:
        files = parse(args.pdf, args.region, args.page_url, args.source_url)
        args.output.mkdir(parents=True)
        for file, data in files.items():
            (args.output / file).write_text(
                json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
            )
    except (OSError, ValueError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    _main()
