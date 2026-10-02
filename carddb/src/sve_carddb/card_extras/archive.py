"""Read pinned regional card pages without live manifest, latest cache or network."""

import re
from datetime import date
from typing import TYPE_CHECKING
from urllib.parse import parse_qsl, urljoin, urlsplit

from sve_carddb.card_extras.models import CardPage, QAEntry, RelatedLink
from sve_carddb.extract.official_en import _qa_text as _en_qa_text
from sve_carddb.extract.official_jp import _qa_text
from sve_carddb.fetch.validate import decode_html
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.html import attribute, parse, require_one, select_all
from sve_carddb.snapshot.values import digest
from sve_carddb.sources import official_en, official_jp

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

    from sve_carddb.build_inputs import Source
    from sve_carddb.registry.records import Region

PARSER = "official-card-extras-jp-v1"
EN_PARSER = "official-card-extras-en-v1"
_TITLE = re.compile(r"^(Q\d+)\s*(?:[（(]([^）)]+)[）)])?$")
_DATE = re.compile(r"^(\d{4})[/-](\d{1,2})[/-](\d{1,2})$")


def card_number(url: str, region: Region) -> str | None:
    """Resolve only an explicit same-region official URL, preserving the raw number."""
    adapter = official_jp if region == "jp" else official_en
    parts = urlsplit(url)
    numbers = [
        value
        for name, value in parse_qsl(parts.query, keep_blank_values=True)
        if name == "cardno"
    ]
    if len(numbers) != 1 or not numbers[0]:
        return None
    expected = urlsplit(adapter.card_url(numbers[0]))
    if (parts.scheme, parts.netloc, parts.path) != (
        expected.scheme,
        expected.netloc,
        expected.path,
    ):
        return None
    return numbers[0]


def parse_card_page(raw: bytes, source: Source, *, region: Region = "jp") -> CardPage:
    """Validate physical identity before transcribing regional supplemental blocks."""
    adapter = official_jp if region == "jp" else official_en
    renderer = _qa_text if region == "jp" else _en_qa_text
    if source.sha256 != digest(raw) or source.parser_version != (
        PARSER if region == "jp" else EN_PARSER
    ):
        raise ValueError("Card extras raw hash/parser pin mismatch")
    number = card_number(source.url, region)
    if (
        number is None
        or source.url != adapter.card_url(number)
        or source.kind != "official_page"
    ):
        raise ValueError("Card extras source URL/region/media mismatch")
    adapter.parse_card(raw, expected_number=number)
    tree = parse(decode_html(raw, min_bytes=official_jp.MIN_PAGE_BYTES))
    nodes = select_all(
        tree,
        ".cardlist-Under .cardlist-Detail_QA .qa-List_Item"
        if region == "jp"
        else ".cardlist-Detail_QA .qa-List_Item",
    )
    if len(nodes) != len(select_all(tree, ".qa-List_Item")):
        raise ValueError("Unrecognized card-page Q&A layout")
    questions: list[QAEntry] = []
    for ordinal, node in enumerate(nodes):
        title = require_one(node, ".qa-List_Ttl").text(strip=True)
        match = _TITLE.fullmatch(title)
        official_number, raw_date = (
            (match[1], match[2]) if match else (None, title or None)
        )
        parsed = _date(raw_date)
        locator = f"qa-block:{ordinal}"
        anchor = attribute(node, "id")
        questions.append(
            QAEntry(
                official_number=official_number,
                stable_source_key=official_number
                or source.url + "#" + (anchor or locator),
                locator=locator,
                question=renderer(require_one(node, ".qa-List_Txt-Q")),
                answer=renderer(require_one(node, ".qa-List_Txt-A")),
                published_on=parsed,
                date_raw=raw_date,
            )
        )
    related = tuple(
        RelatedLink(locator=f"related-link:{ordinal}", href_raw=href)
        for ordinal, node in enumerate(select_all(tree, ".cardlist-Detail_Relation a"))
        if (href := attribute(node, "href")) is not None
    )
    errata = tuple(
        sorted(
            {
                urljoin(source.url, href)
                for node in select_all(tree, ".cardlist-Detail a[href*='/errata/']")
                if (href := attribute(node, "href")) is not None
            }
        )
    )
    return CardPage(
        source=source,
        region=region,
        card_no=number,
        qa=tuple(questions),
        related=related,
        errata_urls=errata,
    )


def _date(raw: str | None) -> str | None:
    match = _DATE.fullmatch(raw or "")
    if match is None:
        return None
    try:
        return date(*map(int, match.groups())).isoformat()
    except ValueError:
        return None


class FrozenCardExtras:
    def __init__(self, root: Path, store_id: str, batch_id: str) -> None:
        self.sources = FrozenSources(root, store_id, batch_id)

    def pages(self) -> Iterator[CardPage]:
        """Stream current JP pages once; each access rechecks raw and metadata hashes."""
        for entry in self.sources.inventory.current:
            source, raw, descriptor = self.sources.read(
                entry.source_version_id, parser_version=PARSER
            )
            if (descriptor.provider, descriptor.kind, descriptor.url) != (
                "jp",
                "card",
                entry.url,
            ):
                raise ValueError("Card extras batch source identity mismatch")
            yield parse_card_page(raw, source)
