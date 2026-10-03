"""Fixed physical/API field projection; historical Git code is never executed."""

import dataclasses
from typing import TYPE_CHECKING
from urllib.parse import parse_qs, urlsplit

from pydantic import JsonValue

from sve_carddb.snapshot.values import canonical, object_value, parse
from sve_carddb.template_semantics.v1 import official_en, official_jp

if TYPE_CHECKING:
    from sve_carddb.translations.models import Span


def excerpt(text: str, span: Span | None) -> str:
    """Offsets are Unicode code points in exact source bytes, never normalized text."""
    if span is None:
        return text
    if not 0 <= span.start < span.end <= len(text):
        raise ValueError("Invalid exact source span")
    return text[span.start : span.end]


def pointer(document: JsonValue, locator: str) -> JsonValue:
    """Resolve a canonical JSON Pointer without executable expressions."""
    if not locator:
        return document
    if not locator.startswith("/"):
        raise ValueError("Evidence locator must be a JSON Pointer")
    current = document
    for encoded in locator[1:].split("/"):
        part = encoded.replace("~1", "/").replace("~0", "~")
        if part.replace("~", "~0").replace("/", "~1") != encoded:
            raise ValueError("Noncanonical JSON Pointer")
        if isinstance(current, dict) and part in current:
            current = current[part]
        elif (
            isinstance(current, list)
            and part.isascii()
            and part.isdecimal()
            and str(int(part)) == part
            and int(part) < len(current)
        ):
            current = current[int(part)]
        else:
            raise ValueError("Evidence JSON Pointer is absent")
    return current


def project(raw: bytes, url: str, provider: str) -> tuple[str, JsonValue]:
    """Keep API JSON unmodified; physical locators use the regional exact extractor."""
    parts = urlsplit(url)
    if provider in {"jp", "en"}:
        numbers = parse_qs(parts.query).get("cardno", [])
        if (
            parts.scheme != "https"
            or parts.netloc
            != (
                "shadowverse-evolve.com"
                if provider == "jp"
                else "en.shadowverse-evolve.com"
            )
            or parts.path != ("/cardlist/" if provider == "jp" else "/cards/")
            or len(numbers) != 1
        ):
            raise ValueError(provider.upper() + " glossary source URL mismatch")
        number = numbers[0]
        card = (
            official_jp.extract_card(raw, number=number)
            if provider == "jp"
            else official_en.extract_card(raw, number=number)
        )
        return ("ja" if provider == "jp" else "en"), parse(
            canonical(dataclasses.asdict(card))
        )
    host = "shadowverse-portal.com" if provider == "sv1" else "shadowverse-wb.com"
    expected = "/api/v1/cards" if provider == "sv1" else "/web/CardList/cardList"
    langs = parse_qs(parts.query).get("lang", [])
    if (
        provider not in {"sv1", "svwb"}
        or parts.scheme != "https"
        or parts.netloc != host
        or parts.path != expected
        or len(langs) != 1
    ):
        raise ValueError("Digital evidence source URL mismatch")
    mapping = (
        {"ja": "ja", "en": "en", "zh-tw": "zh-Hant"}
        if provider == "sv1"
        else {"ja": "ja", "en": "en", "cht": "zh-Hant"}
    )
    if langs[0] not in mapping:
        raise ValueError("Unsupported digital source language")
    data = parse(raw)
    if provider == "sv1":
        if object_value(object_value(data).get("data")).get("errors") != []:
            raise ValueError("Frozen sv1 API reports errors")
    elif object_value(object_value(data).get("data_headers")).get("result_code") != 1:
        raise ValueError("Frozen svwb API reports errors")
    return mapping[langs[0]], data
