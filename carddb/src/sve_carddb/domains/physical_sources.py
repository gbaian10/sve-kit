"""Compact projections of one frozen physical page; raw and DOM stay page-local."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical
from sve_carddb.domains.registry.projection import en_card, jp_card
from sve_carddb.domains.registry.records import Observation
from sve_carddb.domains.registry.review import observation
from sve_carddb.ingest.http.validate import decode_html
from sve_carddb.parse.html import Queryable, parse, select_all, select_one
from sve_carddb.parse.pages import extract_en, extract_jp, official_en, official_jp

if TYPE_CHECKING:
    from collections.abc import Callable

    from sve_carddb.core.provenance import Source
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.card_extras.models import CardPage
    from sve_carddb.domains.products.official import ProductPage
    from sve_carddb.domains.text_observations.models import TextCard
    from sve_carddb.ingest.archive.frozen_sources import FrozenSources
    from sve_carddb.ingest.archive.source_archive import Descriptor


@dataclass(frozen=True, slots=True)
class Projection[T]:
    value: T | None
    error: ValueError | LookupError | None = None

    def get(self) -> T:
        """An unused projection's layout must not impose another adapter's contract."""
        if self.error is not None:
            raise self.error.with_traceback(None)
        assert self.value is not None
        return self.value


def _project[T](extract: Callable[[], T]) -> Projection[T]:
    try:
        return Projection(extract())
    except (ValueError, LookupError) as error:
        # Tracebacks would keep this page's raw bytes and DOM alive in the cache.
        error.__traceback__ = None
        error.__context__ = error.__cause__ = None
        return Projection(None, error)


@dataclass(frozen=True, slots=True)
class PhysicalPage:
    record: Projection[extract_jp.CardRecord | extract_en.CardRecord]
    observation: Projection[Observation]
    text: Projection[TextCard]
    products: Projection[ProductPage]
    extras: Projection[CardPage]
    identity: Projection[bool]

    def require_identity(self) -> None:
        """Products and supplements require the printed physical page identity."""
        self.identity.get()


def page(
    sources: FrozenSources, version: str, *, region: Region, parser_version: str
) -> PhysicalPage:
    """Reuse the full physical extraction recipe within the reader's exact input pin."""
    from sve_carddb.domains.parsed_cards.sources import ParsedSources  # ruff: ignore[import-outside-top-level] -- portable projections reuse the physical domain models

    if isinstance(sources, ParsedSources):
        if sources.selection.region != region:
            raise ValueError("Parsed physical source region mismatch")
        return sources.physical(version, parser_version=parser_version)
    source, _ = sources.metadata(version, parser_version=parser_version)
    return sources.projection(
        version,
        parser_version="physical-page-v1:" + region,
        extract=lambda _source, raw, descriptor: extract(
            source, raw, descriptor, region
        ),
    )


def extract(
    source: Source, raw: bytes, descriptor: Descriptor, region: Region
) -> PhysicalPage:
    """Share the complete DOM recipe between archive reads and portable inputs."""
    from sve_carddb.domains.card_extras import archive as extras  # ruff: ignore[import-outside-top-level] -- avoid regional adapter initialization cycles
    from sve_carddb.domains.products import official as products  # ruff: ignore[import-outside-top-level] -- avoid regional adapter initialization cycles

    if (descriptor.provider, descriptor.kind, source.kind) != (
        region,
        "card",
        "official_page",
    ):
        raise ValueError("Frozen physical page source identity/media mismatch")
    number = extras.card_number(source.url, region)
    if number is None:
        raise ValueError("Frozen physical page URL/region mismatch")
    tree = parse(decode_html(raw, min_bytes=official_jp.MIN_PAGE_BYTES))
    complete = bool(re.search(rb"</body>\s*</html>\s*$", raw, re.IGNORECASE))
    record = _project(lambda: _record(tree, region=region, number=number))
    observed = _project(lambda: _observation(record.get(), region))
    return PhysicalPage(
        record,
        observed,
        _project(
            lambda: _text(
                tree,
                record.get(),
                observed.get(),
                source,
                complete=complete,
            )
        ),
        _project(
            lambda: products.products_tree(
                tree,
                source.model_copy(update={"parser_version": products.PARSER}),
                region,
                number,
            )
        ),
        _project(
            lambda: extras.extras_tree(
                tree,
                source.model_copy(
                    update={
                        "parser_version": extras.PARSER
                        if region == "jp"
                        else extras.EN_PARSER
                    }
                ),
                region=region,
                number=number,
            )
        ),
        _project(lambda: _identity(tree, source, region=region, number=number)),
    )


def _physical(tree: Queryable, *, region: Region, number: str) -> None:
    adapter = official_jp if region == "jp" else official_en
    official_jp.card_tree(tree, expected_number=number, page_dir=adapter.CARD_DIR)


def _identity(tree: Queryable, source: Source, *, region: Region, number: str) -> bool:
    adapter = official_jp if region == "jp" else official_en
    if source.url != adapter.card_url(number):
        raise ValueError("Frozen physical page URL/region mismatch")
    _physical(tree, region=region, number=number)
    return True


def _record(
    tree: Queryable, *, region: Region, number: str
) -> extract_jp.CardRecord | extract_en.CardRecord:
    if region == "jp":
        return extract_jp.extract_tree(tree, number=number)
    # The EN extractor validated the physical page identity before transcribing it.
    _physical(tree, region=region, number=number)
    return extract_en.extract_tree(tree, number=number)


def _observation(
    record: extract_jp.CardRecord | extract_en.CardRecord, region: Region
) -> Observation:
    identity = (
        jp_card(record)
        if isinstance(record, extract_jp.CardRecord)
        else en_card(record)
    )
    return Observation.model_validate_json(canonical(observation(identity, region)))


def _text(
    tree: Queryable,
    record: extract_jp.CardRecord | extract_en.CardRecord,
    observed: Observation,
    source: Source,
    *,
    complete: bool,
) -> TextCard:
    from sve_carddb.domains.text_observations import archive as texts  # ruff: ignore[import-outside-top-level] -- avoid regional adapter initialization cycles
    from sve_carddb.domains.text_observations.models import TextCard  # ruff: ignore[import-outside-top-level] -- avoid regional adapter initialization cycles
    from sve_carddb.domains.text_observations.presence import presence_tree  # ruff: ignore[import-outside-top-level] -- avoid regional adapter initialization cycles

    faces = tuple(
        texts.jp_face(face)
        if isinstance(face, extract_jp.Face)
        else texts.en_face(face)
        for face in record.faces
    )
    card = TextCard(
        source=source,
        observation=observed,
        faces=faces,
        image_src=tuple(face.image for face in record.faces),
        date_raw=record.release_date,
        has_errata_link=record.errata_url is not None,
        effect_presence=tuple(
            presence_tree(
                tree,
                source,
                region=observed.region,
                number=record.number,
                source_index=index,
                complete=complete,
            )
            for index in range(len(faces))
        ),
    )
    whitespace = []
    for node in select_all(tree, ".cardlist-Detail_Box_Inner"):
        container = select_one(node, ".detail")
        value = container.text() if container is not None else None
        whitespace.append(value if value and not value.strip() else None)
    card.accept_extraction(tuple(whitespace))
    return card
