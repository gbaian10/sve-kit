"""Offline regional Q&A observations; absent pagination evidence stays unknown."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal
from urllib.parse import parse_qsl, urljoin, urlsplit

from sve_carddb.card_extras.archive import _TITLE, _date
from sve_carddb.card_extras.models import QABlock, QAEntry, QAPage, RelatedLink
from sve_carddb.extract.official_en import _qa_text as en_text
from sve_carddb.extract.official_jp import _qa_text as jp_text
from sve_carddb.fetch.validate import ValidationError, decode_html
from sve_carddb.html import attribute, parse, require_one, select_all, select_one
from sve_carddb.snapshot.values import digest
from sve_carddb.sources import official_en, official_jp
from sve_carddb.urls import canonicalize

if TYPE_CHECKING:
    from selectolax.lexbor import LexborNode

    from sve_carddb.build_inputs import Source
    from sve_carddb.registry.records import Region

PARSER = "official-qa-observations-v1"


def allowed(url: str, region: Region) -> bool:
    """Restrict requests to the exact regional HTTPS Q&A namespace."""
    parts = urlsplit(url)
    host = official_jp.HOST if region == "jp" else official_en.HOST
    return (
        (parts.scheme, parts.netloc) == ("https", host)
        and parts.path.startswith(("/qa/", "/faq/"))
        and not parts.fragment
    )


@dataclass(frozen=True)
class DetailLink:
    url: str
    href_raw: str
    locator: str


@dataclass(frozen=True)
class Pagination:
    page: int | None
    max_page: int | None
    total: int | None
    urls: tuple[tuple[int, str], ...]


@dataclass(frozen=True)
class ParsedQA:
    blocks: tuple[QABlock, ...]
    details: tuple[DetailLink, ...]
    pagination: Pagination | None
    issues: tuple[str, ...]

    @property
    def listed_count(self) -> int:
        """Count listing positions, including repeated detail associations."""
        return len(self.blocks) + len(self.details)


def _number(raw: str | None, *, zero: bool = False) -> int | None:
    if raw is None:
        return None
    if not raw.isascii() or not raw.isdigit() or int(raw) < (0 if zero else 1):
        raise ValidationError("Invalid Q&A pagination number")
    return int(raw)


def _pagination(node: LexborNode, url: str, region: Region) -> Pagination:
    pager = select_one(node, ".qa-Pager")
    page = _number(attribute(node, "data-page"))
    maximum = _number(attribute(node, "data-max-page"))
    total = _number(attribute(node, "data-total"), zero=True)
    urls: dict[int, str] = {}
    if page is not None:
        urls[page] = url
    if pager is not None:
        for link in select_all(pager, "a[href]"):
            href = attribute(link, "href")
            assert href is not None
            target = canonicalize(urljoin(url, href))
            if not allowed(target, region):
                raise ValidationError(
                    "Q&A pagination points outside the regional source"
                )
            numbers = [
                value
                for name, value in parse_qsl(urlsplit(target).query)
                if name == "page"
            ]
            number = _number(attribute(link, "data-page"))
            if len(numbers) > 1 or (numbers and number != _number(numbers[0])):
                raise ValidationError("Q&A pagination URL/number mismatch")
            if number is None:
                raise ValidationError("Q&A pagination link has no explicit page number")
            if number in urls and urls[number] != target:
                raise ValidationError("Conflicting Q&A pagination targets")
            urls[number] = target
    if len(set(urls.values())) != len(urls):
        raise ValidationError("Repeated Q&A pagination URL")
    return Pagination(page, maximum, total, tuple(sorted(urls.items())))


def _block(
    node: LexborNode, url: str, region: Region, ordinal: int
) -> tuple[QABlock, bool]:
    title = require_one(node, ".qa-List_Ttl").text(strip=True)
    match = _TITLE.fullmatch(title)
    number, raw_date = (match[1], match[2]) if match else (None, title or None)
    anchor = attribute(node, "id")
    locator = f"qa-block:{ordinal}"
    published_raw = attribute(node, "data-published-on")
    updated_raw = attribute(node, "data-updated-on")
    if published_raw is not None or updated_raw is not None:
        raw_date = " | ".join(
            value for value in (raw_date, published_raw, updated_raw) if value
        )
    state = attribute(node, "data-state") or "active"
    if state not in {"active", "withdrawn"}:
        raise ValidationError("Unknown explicit Q&A withdrawal state")
    renderer = jp_text if region == "jp" else en_text
    question = renderer(require_one(node, ".qa-List_Txt-Q"))
    answer = renderer(require_one(node, ".qa-List_Txt-A"))
    if not question or not answer:
        raise ValidationError("Q&A wording is missing")
    entry = QAEntry(
        stable_source_key=number or url + "#" + (anchor or locator),
        official_number=number,
        locator=locator,
        question=question,
        answer=answer,
        published_on=_date(
            published_raw
            if published_raw is not None
            else (match[2] if match else None)
        ),
        updated_on=_date(updated_raw),
        date_raw=raw_date,
        state="withdrawn" if state == "withdrawn" else "active",
    )
    links = tuple(
        RelatedLink(locator=f"{locator}/card-link:{position}", href_raw=href)
        for position, link in enumerate(
            select_all(node, ".qa-List_Cards a, .qa-List_Relation a")
        )
        if (href := attribute(link, "href")) is not None
    )
    return QABlock(entry=entry, card_links=links), number is None and not anchor


def parse_qa(
    raw: bytes, *, url: str, region: Region, kind: Literal["index", "detail"]
) -> ParsedQA:
    """Retain every observed block; require explicit evidence for pagination."""
    if not allowed(url, region):
        raise ValidationError("Q&A URL is outside its regional source")
    tree = parse(decode_html(raw, min_bytes=1))
    root = require_one(tree, ".qa-List")
    nodes = select_all(root, ".qa-List_Item")
    if len(nodes) != len(select_all(tree, ".qa-List_Item")):
        raise ValidationError("Unrecognized Q&A block layout")
    blocks: list[QABlock] = []
    details: list[DetailLink] = []
    issues: list[str] = []
    for ordinal, node in enumerate(nodes):
        question = select_one(node, ".qa-List_Txt-Q")
        answer = select_one(node, ".qa-List_Txt-A")
        if question is not None and answer is not None:
            block, unanchored = _block(node, url, region, ordinal)
            blocks.append(block)
            if unanchored:
                issues.append("unnumbered_identity_reconciliation")
        elif kind == "index" and question is None and answer is None:
            link = require_one(node, ".qa-List_Link[href], .qa-List_Ttl a[href]")
            href = attribute(link, "href")
            assert href is not None
            target = canonicalize(urljoin(url, href))
            if not allowed(target, region):
                raise ValidationError("Q&A detail points outside the regional source")
            details.append(DetailLink(target, href, f"qa-detail:{ordinal}"))
        else:
            raise ValidationError("Incomplete Q&A wording block")
    if kind == "detail" and not blocks:
        raise ValidationError("Q&A detail contains no observed question")
    pagination = _pagination(root, url, region) if kind == "index" else None
    if kind == "index" and not nodes and select_one(root, ".qa-Empty") is None:
        issues.append("unknown_empty_layout")
    return ParsedQA(tuple(blocks), tuple(details), pagination, tuple(issues))


def materialize(
    raw: bytes, source: Source, *, region: Region, kind: Literal["index", "detail"]
) -> QAPage:
    """Bind parsed observations to verified frozen source metadata."""
    if source.sha256 != digest(raw) or source.parser_version != PARSER:
        raise ValueError("Q&A raw hash/parser pin mismatch")
    parsed = parse_qa(raw, url=source.url, region=region, kind=kind)
    return QAPage(source=source, region=region, blocks=parsed.blocks)
