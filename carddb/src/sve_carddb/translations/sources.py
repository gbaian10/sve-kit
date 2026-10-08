"""Resolve exact glossary evidence from verified frozen sources."""

import dataclasses
from typing import TYPE_CHECKING
from urllib.parse import parse_qs, urlsplit

from pydantic import JsonValue

from sve_carddb.build_inputs import SourceUse
from sve_carddb.catalog.adoption_sources import AdoptionSources
from sve_carddb.extract import official_en, official_jp
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.snapshot.values import canonical, digest, object_value, parse

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_inputs import BuildContext, Source
    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.digital_links.evidence import RegistryIndex
    from sve_carddb.registry.snapshot import RegistrySnapshot
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


class Sources:
    def __init__(
        self,
        stores: dict[str, Path],
        repository: Path,
        build: BuildContext,
        registry: RegistrySnapshot | None = None,
    ) -> None:
        self.stores = stores
        self.repository = repository
        self._build = build
        self.identities = AdoptionSources(stores, repository, registry)
        self.identity_indexes: dict[bytes, RegistryIndex] = {}
        self.context_keys: dict[BuildContext, bytes] = {}
        self.batches: dict[str, FrozenSources] = {}
        self.cache: dict[tuple[str, str, str], tuple[str, JsonValue, Source]] = {}
        self.uses: list[SourceUse] = []

    @property
    def build(self) -> BuildContext:
        """One stage keeps its construction context for every owner check."""
        return self._build

    def stage(self, build: BuildContext) -> Sources:
        """Keep identity reads and source uses stage-local.

        Share identity indexes, decoded projections, context keys and verified batches.
        """
        stage = Sources(
            self.stores, self.repository, build, self.identities.current_registry
        )
        stage.identity_indexes = self.identity_indexes
        stage.context_keys = self.context_keys
        stage.batches = self.batches
        stage.cache = self.cache
        return stage

    def batch(self, batch_id: str) -> FrozenSources:
        """Verify the configured archive ownership before exposing any source member."""
        if batch_id not in self.batches:
            self.batches[batch_id] = FrozenSources.configured(self.stores, batch_id)
        return self.batches[batch_id]

    def context_key(self, context: BuildContext) -> bytes:
        """Full immutable contexts distinguish inputs without quoting them for every owner."""
        if context not in self.context_keys:
            self.context_keys[context] = canonical(context.model_dump(mode="json"))
        return self.context_keys[context]

    def document(self, ref: SourceRef) -> tuple[str, JsonValue, Source]:
        """Verify recipe, archive membership, metadata and raw before resolving text."""
        return self.projection(ref.batch_id, ref.source_version_id, ref.parser)

    def projection(
        self, batch_id: str, version: str, parser: str
    ) -> tuple[str, JsonValue, Source]:
        """Replay a complete page without inventing a text locator or text hash."""
        providers = {
            "translation-" + provider + "-v1": provider
            for provider in ("jp", "en", "sv1", "svwb")
        }
        provider = providers.get(parser)
        if provider is None:
            raise ValueError("Unsupported translation source recipe")
        cache_key = (batch_id, version, parser)
        if cache_key not in self.cache:
            source, raw, descriptor = self.batch(batch_id).read(
                version, parser_version=parser
            )
            if descriptor.provider != provider or descriptor.kind != (
                "card" if provider in {"jp", "en"} else "api"
            ):
                raise ValueError("Frozen evidence provider/kind mismatch")
            lang, document = project(raw, source.url, provider)
            self.cache[cache_key] = lang, document, source
        return self.cache[cache_key]

    def text(self, ref: SourceRef, span: Span | None = None) -> tuple[str, str, Source]:
        """Verify a complete exact field before extracting a bounded span."""
        lang, document, source = self.document(ref)
        value = pointer(document, ref.locator)
        if not isinstance(value, str) or digest(value.encode()) != ref.text_hash:
            raise ValueError("Evidence must locate exact hash-verified text")
        self.uses.append(
            SourceUse(source=source, usage="translation_evidence", locator=ref.locator)
        )
        return lang, excerpt(value, span), source
