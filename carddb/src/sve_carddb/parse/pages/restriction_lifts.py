"""Observe explicit JP Standard lift sections without inferring missing history."""

import re
from datetime import date

from sve_carddb.core.regions import SourceRegion
from sve_carddb.parse.html import attribute, parse, require_one, select_all
from sve_carddb.parse.pages.restriction_list import linked_card_number

_HEADING = re.compile(
    r"■([0-9]{4})年([0-9]{1,2})月([0-9]{1,2})日\([^)]*\)から殿堂入り解除になるカード"
)


def parse_jp_restriction_lifts(body: bytes) -> list[dict[str, str]]:  # ruff: ignore[complex-structure] -- Keep the bounded lift-section admission together.
    """Read only the explicit Standard lift block of the supported notice layout."""
    if not body.rstrip().lower().endswith(b"</html>"):
        raise ValueError("Truncated JP restriction lift notice")
    document = parse(body.decode("utf-8"))
    title = require_one(document, ".news_Detail .heading h1").text(strip=True)
    if not title.startswith("「スタンダード」") or "殿堂入り解除" not in title:
        raise ValueError("JP lift notice must explicitly identify Standard")
    container = require_one(document, ".news_Detail .contents")
    lifted_on: str | None = None
    entries: list[dict[str, str]] = []
    headings = 0
    for ordinal, node in enumerate(select_all(container, "p,h2,a[href]"), 1):
        text = node.text(separator="", strip=True).strip()
        if node.tag in {"p", "h2"} and (text.startswith("■") or node.tag == "h2"):
            lifted_on = None
            if match := _HEADING.fullmatch(text):
                headings += 1
                lifted_on = date(*(int(part) for part in match.groups())).isoformat()
            elif text.startswith("■") and "解除になるカード" in text:
                raise ValueError("Unsupported JP restriction lift date")
        if node.tag != "a" or lifted_on is None:
            continue
        href = attribute(node, "href")
        card_no = linked_card_number(href or "", SourceRegion.JP)
        if not card_no or not text.startswith("『") or not text.endswith("』"):
            raise ValueError("JP lifted card requires its linked number and name")
        if len(select_all(node, "*")) > 1 or not text[1:-1].strip():
            raise ValueError("Unsupported JP lifted card name")
        entries.append(
            {
                "name": text[1:-1],
                "card_no": card_no,
                "card_link": href or "",
                "effective_until": lifted_on,
                "locator": f"css:.news_Detail .contents;query=p,h2,a[href];ordinal={ordinal}",
            }
        )
    if (
        headings != 1
        or not entries
        or len({item["card_no"] for item in entries}) != len(entries)
    ):
        raise ValueError("JP lift notice requires one nonempty, unique lift section")
    return entries
