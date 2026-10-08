"""Resolve supported source fields from sealed inputs, never from a latest cache."""

import dataclasses
import re
from typing import TYPE_CHECKING
from urllib.parse import parse_qs, urlsplit

from pydantic import JsonValue

from sve_carddb.build_inputs import SourceUse
from sve_carddb.catalog.adoption_models import (
    AuthoredText,
    ImageEvidence,
    SourceRef,
    SourceText,
    TextEvidence,
)
from sve_carddb.extract import official_en, official_jp
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.products.models import LocalizedText
from sve_carddb.registry.inputs import JSON_VALUE
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.snapshot.values import canonical, digest, parse

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.build_inputs import Source
    from sve_carddb.catalog.adoption_models import Record, ReviewContext, TextValue
    from sve_carddb.catalog.current_models import (
        VocabularyRecord as CurrentVocabularyRecord,
    )
    from sve_carddb.registry.snapshot import RegistrySnapshot

SOURCE_PARSERS = {"official-jp-exact-v1", "official-en-exact-v1", "exact-json-v1"}


class AdoptionSources:
    def __init__(
        self,
        stores: Mapping[str, Path],
        repository: Path,
        registry: RegistrySnapshot | None = None,
    ) -> None:
        self.stores = dict(stores)
        self.repository = repository
        self.batches: dict[str, FrozenSources] = {}
        self.uses: list[SourceUse] = []
        self.cache: dict[bytes, tuple[LocalizedText, Source, JsonValue]] = {}
        self.current_registry = registry

    def registry(self) -> RegistrySnapshot:
        """Reuse the current registry; Git history cannot establish owner applicability."""
        if self.current_registry is None:
            self.current_registry = load_registry(self.repository / "authored")
        return self.current_registry

    def batch(self, batch: str) -> FrozenSources:
        """Validate the complete descriptor/receipt/raw closure once per frozen batch."""
        if batch not in self.batches:
            self.batches[batch] = FrozenSources.configured(self.stores, batch)
        return self.batches[batch]

    def text(
        self, ref: SourceRef, review: ReviewContext
    ) -> tuple[LocalizedText, Source, JsonValue]:
        """Resolve JSON Pointer and hash the exact nonempty UTF-8 string."""
        key = canonical([ref.model_dump(mode="json"), review.model_dump(mode="json")])
        if key not in self.cache:
            if ref.parser not in SOURCE_PARSERS:
                raise ValueError("Unsupported source parser recipe")
            source, raw, descriptor = self.batch(ref.batch_id).read(
                ref.source_version_id,
                parser_version=ref.parser,
            )
            expected_provider = {
                "official-jp-exact-v1": "jp",
                "official-en-exact-v1": "en",
            }.get(ref.parser)
            if expected_provider is not None and (
                descriptor.provider,
                descriptor.kind,
            ) != (expected_provider, "card"):
                raise ValueError("Catalog source provider/kind mismatch")
            projection = self._projection(ref.parser, raw, descriptor.url)
            value = pointer(projection, ref.locator)
            if (
                not isinstance(value, str)
                or not value
                or digest(value.encode()) != ref.text_hash
            ):
                raise ValueError("Source locator/exact text hash mismatch")
            if descriptor.provider not in {"jp", "en"}:
                raise ValueError("Source language cannot be determined")
            lang = "ja" if descriptor.provider == "jp" else "en"
            self.cache[key] = LocalizedText(lang=lang, text=value), source, projection
            self._use(source, "catalog_exact_text", ref.model_dump(mode="json"))
        return self.cache[key]

    @staticmethod
    def _projection(parser: str, raw: bytes, url: str) -> JsonValue:
        return _projection(parser, raw, url)

    def value(
        self,
        value: TextValue,
        record: Record | CurrentVocabularyRecord,
        review: ReviewContext,
    ) -> LocalizedText:
        """Source TextValues must be part of the same human-approved evidence set."""
        if isinstance(value, AuthoredText):
            return LocalizedText(lang=value.lang, text=value.text)
        assert isinstance(value, SourceText)
        if not any(
            isinstance(e, TextEvidence) and e.source_ref == value.source_ref
            for e in record.evidence
        ):
            raise ValueError("Source TextValue is absent from adoption evidence")
        return self.text(value.source_ref, review)[0]

    def image(self, evidence: ImageEvidence) -> Source:
        """Keep image raw hashes separate from string hashes and parser recipes."""
        ref = evidence.image_ref
        source, _, _ = self.batch(ref.batch_id).read(
            ref.source_version_id,
            parser_version="catalog-image-closure-v1",
        )
        if source.kind != "image" or source.sha256 != ref.raw_hash:
            raise ValueError("Adoption image descriptor/raw hash mismatch")
        self._use(source, "catalog_image_evidence", ref.model_dump(mode="json"))
        return source

    def _use(self, source: Source, usage: str, locator: JsonValue) -> None:
        self.uses.append(
            SourceUse(source=source, usage=usage, locator=canonical(locator).decode())
        )


def pointer(value: JsonValue, locator: str) -> JsonValue:
    """Use RFC 6901 indexing only; no executable expressions or implicit normalization."""
    if not locator:
        return value
    if not locator.startswith("/"):
        raise ValueError("Invalid adoption JSON Pointer")
    for segment in locator.split("/")[1:]:
        if re.search(r"~(?![01])", segment):
            raise ValueError("Invalid adoption JSON Pointer escape")
        key = segment.replace("~1", "/").replace("~0", "~")
        if isinstance(value, dict) and key in value:
            value = value[key]
        elif (
            isinstance(value, list)
            and key.isascii()
            and key.isdecimal()
            and str(int(key)) == key
            and int(key) < len(value)
        ):
            value = value[int(key)]
        else:
            raise ValueError("Adoption source locator is absent")
    return value


def _projection(parser: str, raw: bytes, url: str) -> JsonValue:
    if parser not in SOURCE_PARSERS:
        raise ValueError("Unsupported source parser recipe")
    if parser == "exact-json-v1":
        return parse(raw)
    number = parse_qs(urlsplit(url).query).get("cardno", [])
    if len(number) != 1:
        raise ValueError("Card source lacks exact official number")
    result = (
        official_jp.extract_card(raw, number=number[0])
        if parser == "official-jp-exact-v1"
        else official_en.extract_card(raw, number=number[0])
    )
    return JSON_VALUE.validate_python(dataclasses.asdict(result), strict=True)
