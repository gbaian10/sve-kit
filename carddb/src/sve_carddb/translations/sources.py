"""Replay exact glossary evidence from sealed sources and pinned parser bytes."""

import dataclasses
from pathlib import Path
from typing import TYPE_CHECKING
from urllib.parse import parse_qs, urlsplit

from pydantic import JsonValue

from sve_carddb.build_inputs import SourceUse
from sve_carddb.catalog.adoption_models import Normalizer
from sve_carddb.catalog.adoption_sources import AdoptionSources, PinnedRepository
from sve_carddb.extract import official_jp
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.snapshot.values import canonical, digest, object_value, parse

if TYPE_CHECKING:
    from sve_carddb.build_inputs import BuildContext, Source
    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.digital_links.evidence import RegistryIndex
    from sve_carddb.translations.models import Span

CODE_PATH = "carddb/src/sve_carddb/translations/sources.py"
RUNTIME = (
    "carddb/uv.lock",
    "carddb/pyproject.toml",
    CODE_PATH,
    "carddb/src/sve_carddb/translations/digital.py",
    "carddb/src/sve_carddb/translations/importer.py",
    "carddb/src/sve_carddb/translations/loader.py",
    "carddb/src/sve_carddb/translations/models.py",
    "carddb/src/sve_carddb/translations/names.py",
    "carddb/src/sve_carddb/catalog/adoption_sources.py",
    "carddb/src/sve_carddb/extract/official_jp.py",
    "carddb/src/sve_carddb/sources/official_jp.py",
    "carddb/src/sve_carddb/html.py",
    "carddb/src/sve_carddb/fetch/validate.py",
    "carddb/src/sve_carddb/snapshot/values.py",
    "carddb/src/sve_carddb/frozen_sources.py",
    "carddb/src/sve_carddb/source_archive.py",
    "carddb/src/sve_carddb/build_inputs.py",
    "carddb/src/sve_carddb/text_observations/intern.py",
    "carddb/src/sve_carddb/catalog/importer.py",
    "carddb/src/sve_carddb/digital_links/candidates.py",
    "carddb/src/sve_carddb/digital_links/commands.py",
    "carddb/src/sve_carddb/catalog/adoption_models.py",
    "carddb/src/sve_carddb/catalog/adoption_loader.py",
    "carddb/src/sve_carddb/products/models.py",
    "carddb/src/sve_carddb/digital_links/models.py",
    "carddb/src/sve_carddb/digital_links/loader.py",
    "carddb/src/sve_carddb/digital_links/evidence.py",
    "carddb/src/sve_carddb/digital_links/importer.py",
    "carddb/src/sve_carddb/registry/snapshot.py",
    "carddb/src/sve_carddb/registry/storage.py",
    "carddb/src/sve_carddb/registry/records.py",
    "carddb/src/sve_carddb/registry/validate.py",
    "carddb/src/sve_carddb/registry/inputs.py",
    "carddb/src/sve_carddb/registry/allocation.py",
    "carddb/src/sve_carddb/registry/yaml_reader.py",
    "carddb/src/sve_carddb/registry/transitions/files.py",
)


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
    """Keep API JSON unmodified; JP locators address the existing exact extractor."""
    parts = urlsplit(url)
    if provider == "jp":
        numbers = parse_qs(parts.query).get("cardno", [])
        if (
            parts.scheme != "https"
            or parts.netloc != "shadowverse-evolve.com"
            or parts.path != "/cardlist/"
            or len(numbers) != 1
        ):
            raise ValueError("JP glossary source URL mismatch")
        number = numbers[0]
        card = official_jp.extract_card(raw, number=number)
        return "ja", parse(canonical(dataclasses.asdict(card)))
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
        *,
        historical: bool = False,
    ) -> None:
        self.stores = stores
        self.repository = PinnedRepository(repository)
        self.build = build
        self.historical = historical
        self.repository.context(build)
        self.identities = AdoptionSources(stores, self.repository)
        self.identity_indexes: dict[bytes, RegistryIndex] = {}
        dependencies = {pin.name: pin.sha256 for pin in build.dependencies}
        runtime = Path(__file__).resolve().parents[4]
        for name in () if historical else RUNTIME:
            file = runtime / name
            if file.is_symlink() or dependencies.get(name) != digest(file.read_bytes()):
                raise ValueError(
                    "Translation runtime/dependency closure cannot be replayed"
                )
        self.batches: dict[tuple[str, str], FrozenSources] = {}
        self.cache: dict[tuple[str, str, str, str], tuple[str, JsonValue, Source]] = {}
        self.uses: list[SourceUse] = []

    def document(self, ref: SourceRef) -> tuple[str, JsonValue, Source]:
        """Verify recipe, archive membership, metadata and raw before resolving text."""
        return self.projection(
            ref.store_id, ref.batch_id, ref.source_version_id, ref.parser
        )

    def projection(
        self, store_id: str, batch_id: str, version: str, parser: str
    ) -> tuple[str, JsonValue, Source]:
        """Replay a complete page without inventing a text locator or text hash."""
        config = object_value(parse(self.build.configuration.encode()))
        recipes = object_value(config.get("translation_recipes"))
        pin = Normalizer.model_validate_json(canonical(recipes.get(parser)))
        if (
            pin.version != parser
            or pin.code_path != CODE_PATH
            or set(pin.config) != {"provider"}
        ):
            raise ValueError("Unsupported translation source recipe")
        self.repository.implementation(
            pin, self.build, current_runtime=not self.historical
        )
        provider = pin.config["provider"]
        if not isinstance(provider, str) or parser != "translation-" + provider + "-v1":
            raise ValueError("Translation recipe/provider mismatch")
        cache_key = (store_id, batch_id, version, parser)
        if cache_key not in self.cache:
            batch_key = (store_id, batch_id)
            if batch_key not in self.batches:
                self.batches[batch_key] = FrozenSources(
                    self.stores[store_id], *batch_key
                )
            source, raw, descriptor = self.batches[batch_key].read(
                version, parser_version=parser
            )
            if descriptor.provider != provider or descriptor.kind != (
                "card" if provider == "jp" else "api"
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
