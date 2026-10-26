"""Observe current Standard list entries without adopting their rule identities."""

import re
from dataclasses import dataclass, replace
from datetime import date
from typing import TYPE_CHECKING, Literal
from urllib.parse import parse_qs, urlsplit

from sve_carddb.core.regions import SourceRegion
from sve_carddb.parse.html import attribute, parse, require_one, select_all

if TYPE_CHECKING:
    from selectolax.lexbor import LexborNode

_JP_DATE = re.compile(r"([0-9]{4})/([0-9]{1,2})/([0-9]{1,2})\([^)]*\)より適用")
_EN_DATE = re.compile(
    r"Effective as of ([A-Za-z]+) ([0-9]{1,2}), "
    r"(?:to be lifted on ([A-Za-z]+) ([0-9]{1,2}), )?([0-9]{4})"
)
_EN_LIFT = re.compile(r"\(to be lifted on ([A-Za-z]+) ([0-9]{1,2}), ([0-9]{4})\)")
_MONTHS = (
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
)
_TITLES = {
    SourceRegion.JP: "カードの使用制限に関するルール",
    SourceRegion.EN: "Restrictions",
}
_JP_CATEGORIES = {"禁止カード": 0, "殿堂入りカード": 1}
_EN_CATEGORIES = {"Banned": 0, "Restricted to 1": 1}


@dataclass(frozen=True, slots=True)
class ListRestriction:
    name: str
    card_no: str | None
    card_link: str
    max_copies: int
    effective_from: date
    effective_until: date | None
    heading: str
    locator: str

    def active_on(self, day: date) -> bool:
        """A restriction lifted on a date is already inactive on that date."""
        return self.effective_from <= day and (
            self.effective_until is None or day < self.effective_until
        )


@dataclass(frozen=True, slots=True)
class RestrictionList:
    region: SourceRegion
    entries: tuple[ListRestriction, ...]
    excluded_sections: tuple[str, ...]
    semantic_state: Literal["unreviewed"] = "unreviewed"


def _month(name: str) -> int:
    try:
        return _MONTHS.index(name) + 1
    except ValueError:
        raise ValueError("Unsupported EN restriction month") from None


def _dates(heading: str, region: SourceRegion) -> tuple[date, date | None]:
    if region is SourceRegion.JP:
        match = _JP_DATE.fullmatch(heading)
        if match is None:
            raise ValueError("Unsupported JP restriction effective-date heading")
        return date(*(int(value) for value in match.groups())), None
    match = _EN_DATE.fullmatch(heading)
    if match is None:
        raise ValueError("Unsupported EN restriction effective-date heading")
    month, day, end_month, end_day, year = match.groups()
    start = date(int(year), _month(month), int(day))
    end = (
        None if end_month is None else date(int(year), _month(end_month), int(end_day))
    )
    if end is not None and end <= start:
        raise ValueError("Restriction effective-date interval must be nonempty")
    return start, end


def linked_card_number(href: str, region: SourceRegion) -> str | None:
    """Validate a regional official link without deriving a number from its name."""
    parts = urlsplit(href)
    host = (
        "shadowverse-evolve.com"
        if region is SourceRegion.JP
        else "en.shadowverse-evolve.com"
    )
    if parts.scheme != "https" or parts.netloc != host or parts.fragment:
        raise ValueError("Restriction card link must use its regional official host")
    if region is SourceRegion.EN and re.fullmatch(
        r"/news/post-[1-9][0-9]*/?", parts.path
    ):
        return None
    path = "/cardlist/" if region is SourceRegion.JP else "/cards/"
    values = parse_qs(parts.query, keep_blank_values=True).get("cardno", [])
    if parts.path != path or len(values) != 1 or not values[0].strip():
        raise ValueError("Restriction entry lacks a unique linked card number")
    return values[0].strip()


def _inline_lift(node: LexborNode, entry: ListRestriction) -> ListRestriction:
    container = node.parent
    name = node.text(separator="", strip=True).strip()
    end = entry.effective_until
    while container is not None and container.tag in {"strong", "span", "em", "b", "p"}:
        text = container.text(separator="", strip=True).strip()
        if text != name:
            suffix = text.removeprefix(name).strip()
            match = _EN_LIFT.fullmatch(suffix) if text.startswith(name) else None
            if match is None or len(select_all(container, "a[href]")) != 1:
                raise ValueError("Unsupported EN restriction inline annotation")
            month, day, year = match.groups()
            lifted = date(int(year), _month(month), int(day))
            if lifted <= entry.effective_from:
                raise ValueError("Restriction effective-date interval must be nonempty")
            if end is not None and end != lifted:
                raise ValueError("Restriction has conflicting lift dates")
            end = lifted
        container = container.parent
    return replace(entry, effective_until=end)


@dataclass(slots=True)
class _Cursor:
    standard: bool = False
    max_copies: int | None = None
    heading: str = ""
    dates: tuple[date, date | None] | None = None
    date_entries: int = 0

    def finish_heading(self) -> None:
        if self.dates is not None and not self.date_entries:
            raise ValueError("Restriction effective-date heading has no entries")

    def category(self, max_copies: int | None) -> None:
        self.finish_heading()
        self.max_copies = max_copies
        self.heading = ""
        self.dates = None
        self.date_entries = 0

    def section(self, text: str, region: SourceRegion) -> str | None:
        if region is SourceRegion.JP:
            self.standard = text in _JP_CATEGORIES
            self.category(_JP_CATEGORIES.get(text))
            allowed = {"過去の更新内容", "「クロスオーバー」のカードの使用制限"}
        else:
            self.standard = text == "Standard"
            self.category(None)
            allowed = {"Cross Craft", "Gloryfinder"}
        if self.standard:
            return None
        if text not in allowed:
            raise ValueError("Unsupported restriction section or format")
        return text

    def effective(self, text: str, region: SourceRegion) -> None:
        if self.max_copies is None:
            raise ValueError("Restriction date precedes its category")
        self.finish_heading()
        self.heading, self.dates = text, _dates(text, region)
        self.date_entries = 0

    def entry(
        self, text: str, href: str, region: SourceRegion, ordinal: int
    ) -> ListRestriction:
        if self.max_copies is None or self.dates is None or not text:
            raise ValueError("Restriction entry lacks category, date or name")
        if region is SourceRegion.JP:
            if not text.startswith("『") or not text.endswith("』"):
                raise ValueError("JP restriction name lacks its expected quotation")
            text = text[1:-1]
        if not text.strip():
            raise ValueError("Restriction entry name must not be blank")
        result = ListRestriction(
            text,
            linked_card_number(href, region),
            href,
            self.max_copies,
            *self.dates,
            self.heading,
            f"css:.sw-Lower_Container;query=h2,h3,h4,p,a[href];ordinal={ordinal}",
        )
        self.date_entries += 1
        return result

    def consume(
        self, node: LexborNode, region: SourceRegion, ordinal: int
    ) -> ListRestriction | None:
        text = node.text(separator="", strip=True).strip()
        if region is SourceRegion.EN and node.tag == "p" and text in _EN_CATEGORIES:
            self.category(_EN_CATEGORIES[text])
        if (
            region is SourceRegion.EN
            and node.tag == "p"
            and "lifted" in text.lower()
            and not select_all(node, "a[href]")
        ):
            raise ValueError("Unattributed EN restriction lift annotation")
        if node.tag == "h4":
            self.effective(text, region)
        if node.tag == "a":
            if len(select_all(node, "*")) > 1:
                raise ValueError("Restriction name contains unsupported nested markup")
            href = attribute(node, "href")
            assert href is not None
            entry = self.entry(text, href, region, ordinal)
            return _inline_lift(node, entry) if region is SourceRegion.EN else entry
        return None


def _section(body: bytes, region: SourceRegion) -> list[tuple[int, LexborNode]]:
    document = parse(body.decode("utf-8"))
    container = require_one(document, ".sw-Lower_Container")
    selected: list[tuple[int, LexborNode]] = []
    entered = False
    for ordinal, node in enumerate(select_all(container, "h2,h3,h4,p,a[href]"), 1):
        if node.tag == "h2":
            if entered:
                break
            entered = node.text(strip=True).strip() == _TITLES[region]
        elif entered:
            selected.append((ordinal, node))
    if not entered:
        raise ValueError("Restriction list lacks its regional title")
    return selected


def parse_restriction_list(body: bytes, region: SourceRegion) -> RestrictionList:
    """Parse only the two list layouts; all semantic adoption remains human-reviewed."""
    if region not in _TITLES or not body.rstrip().lower().endswith(b"</html>"):
        raise ValueError("Unsupported or truncated restriction list")
    cursor = _Cursor()
    seen: set[int] = set()
    entries: list[ListRestriction] = []
    excluded: list[str] = []
    for ordinal, node in _section(body, region):
        text = node.text(separator="", strip=True).strip()
        if node.tag == "h3" and (omitted := cursor.section(text, region)) is not None:
            excluded.append(omitted)
        if not cursor.standard:
            continue
        if (entry := cursor.consume(node, region, ordinal)) is not None:
            entries.append(entry)
            seen.add(entry.max_copies)
    cursor.finish_heading()
    if seen != {0, 1} or not entries:
        raise ValueError("Restriction list lacks both Standard categories")
    identities = [
        (entry.name, entry.max_copies, entry.effective_from, entry.effective_until)
        for entry in entries
    ]
    if len(identities) != len(set(identities)):
        raise ValueError("Restriction list repeats an entry")
    names = [entry.name for entry in entries]
    numbers = [entry.card_no for entry in entries if entry.card_no is not None]
    if len(names) != len(set(names)) or len(numbers) != len(set(numbers)):
        raise ValueError(
            "Restriction list has conflicting entries for one name or card number"
        )
    return RestrictionList(region, tuple(entries), tuple(excluded))
