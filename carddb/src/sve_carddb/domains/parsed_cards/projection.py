"""Persist DOM-only evidence while sharing the existing downstream projections."""

from dataclasses import asdict
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, digest
from sve_carddb.domains import physical_sources
from sve_carddb.domains.card_extras.archive import EN_PARSER, PARSER
from sve_carddb.domains.card_extras.models import CardPage, QAEntry
from sve_carddb.domains.parsed_cards.models import (
    ENDocument,
    Evidence,
    JPDocument,
    LayoutEvidence,
    ParsedObservation,
    ProductEvidence,
    QAIdentity,
)
from sve_carddb.domains.products import official as products
from sve_carddb.domains.registry.projection import en_card, jp_card
from sve_carddb.domains.registry.records import Observation
from sve_carddb.domains.registry.review import observation
from sve_carddb.domains.text_observations.archive import en_face, jp_face
from sve_carddb.domains.text_observations.models import TextCard
from sve_carddb.parse.pages import extract_jp

if TYPE_CHECKING:
    from sve_carddb.core.provenance import Source
    from sve_carddb.core.regions import Region
    from sve_carddb.ingest.archive.source_archive import Descriptor


def extract(
    raw: bytes,
    source: Source,
    descriptor: Descriptor,
    *,
    region: Region,
    producer_revision: str,
) -> ParsedObservation:
    """Validate original bytes before admitting any portable parse evidence."""
    if digest(raw) != source.sha256 or descriptor.id != source.id:
        raise ValueError("Parsed input differs from the pinned original")
    page = physical_sources.extract(source, raw, descriptor, region)
    page.require_identity()
    record, text, extras = page.record.get(), page.text.get(), page.extras.get()
    document = (
        JPDocument(region="jp", record=record)
        if isinstance(record, extract_jp.CardRecord)
        else ENDocument(region="en", record=record)
    )
    evidence = Evidence(
        descriptor=descriptor,
        descriptor_sha256=source.archive.descriptor_sha256,
        fetched_at=source.fetched_at,
        etag=source.etag,
        last_modified=source.last_modified,
    )
    layout = LayoutEvidence(
        presence=text.effect_presence,
        whitespace=tuple(
            projected if projected and not projected.strip() else None
            for i in range(len(text.faces))
            for projected in (text.projected(i).effect,)
        ),
        products=tuple(
            ProductEvidence.model_validate(asdict(block))
            for block in page.products.get().blocks
        ),
        qa=tuple(
            QAIdentity(
                stable_source_key=q.stable_source_key,
                official_number=q.official_number,
                locator=q.locator,
                published_on=q.published_on,
                date_raw=q.date_raw,
            )
            for q in extras.qa
        ),
        related=extras.related,
        errata_urls=extras.errata_urls,
    )
    candidate = ParsedObservation.model_construct(
        producer_revision=producer_revision,
        evidence=evidence,
        document=document,
        layout=layout,
        content_hash="sha256:" + "0" * 64,
    )
    return ParsedObservation.model_validate_json(
        candidate.model_copy(
            update={"content_hash": candidate.calculated_hash()}
        ).model_dump_json()
    )


def replay(value: ParsedObservation, source: Source) -> physical_sources.PhysicalPage:
    """Reconstruct domain inputs without parsing or synthesizing HTML."""
    if (source.id, source.sha256, source.url) != (
        value.evidence.descriptor.id,
        value.evidence.descriptor.raw_sha256,
        value.evidence.descriptor.url,
    ):
        raise ValueError("Parsed replay source differs from reviewed evidence")
    record, region = value.document.record, value.document.region
    identity = (
        jp_card(record)
        if isinstance(record, extract_jp.CardRecord)
        else en_card(record)
    )
    observed = Observation.model_validate_json(canonical(observation(identity, region)))
    text = TextCard(
        source=source,
        observation=observed,
        faces=tuple(
            jp_face(f) if isinstance(f, extract_jp.Face) else en_face(f)
            for f in record.faces
        ),
        image_src=tuple(f.image for f in record.faces),
        date_raw=record.release_date,
        has_errata_link=record.errata_url is not None,
        effect_presence=value.layout.presence,
    )
    text.accept_extraction(value.layout.whitespace)
    extras = CardPage(
        source=source.model_copy(
            update={"parser_version": PARSER if region == "jp" else EN_PARSER}
        ),
        region=region,
        card_no=record.number,
        qa=tuple(
            QAEntry(**identity.model_dump(), question=question, answer=answer)
            for identity, (question, answer) in zip(
                value.layout.qa,
                tuple((q.question, q.answer) for q in record.qa),
                strict=True,
            )
        ),
        related=value.layout.related,
        errata_urls=value.layout.errata_urls,
    )
    return physical_sources.PhysicalPage(
        physical_sources.Projection(record),
        physical_sources.Projection(observed),
        physical_sources.Projection(text),
        physical_sources.Projection(
            products.ProductPage(
                source.model_copy(update={"parser_version": products.PARSER}),
                region,
                record.number,
                tuple(
                    products.ProductBlock(
                        block.ordinal,
                        block.name,
                        block.date_raw,
                        block.href_raw,
                        block.resolved_urls,
                        block.matches,
                        block.diagnostics,
                    )
                    for block in value.layout.products
                ),
            )
        ),
        physical_sources.Projection(extras),
        physical_sources.Projection(True),
    )
