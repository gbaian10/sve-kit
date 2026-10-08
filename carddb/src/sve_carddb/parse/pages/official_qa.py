"""Pure Q&A URL and pagination parsing, without adopted records."""

from dataclasses import dataclass
from typing import TYPE_CHECKING
from urllib.parse import parse_qsl, urljoin, urlsplit

from sve_carddb.ingest.http.validate import ValidationError
from sve_carddb.ingest.urls import canonicalize
from sve_carddb.parse.html import attribute, select_all, select_one
from sve_carddb.parse.pages import official_en, official_jp

if TYPE_CHECKING:
    from selectolax.lexbor import LexborNode

    from sve_carddb.core.regions import Region


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


def _number(raw: str | None, *, zero: bool = False) -> int | None:
    if raw is None:
        return None
    if not raw.isascii() or not raw.isdigit() or int(raw) < (0 if zero else 1):
        raise ValidationError("Invalid Q&A pagination number")
    return int(raw)


def pagination(node: LexborNode, url: str, region: Region) -> Pagination:
    """Retain only explicitly numbered, same-region pagination targets."""
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
