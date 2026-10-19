"""Read the admitted card-data tree; generation and validate own integrity checks."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.parsed_cards.models import (
    BatchFile,
    BatchSelection,
    Card,
    ENDocument,
    Evidence,
    FilePair,
    Index,
    JPDocument,
    LayoutEvidence,
    ParsedObservation,
    ProductEvidence,
)
from sve_carddb.domains.parsed_cards.storage import Catalog, member
from sve_carddb.domains.parsed_cards.trusted import construct, decode
from sve_carddb.domains.parsed_cards.wire import (
    CardFile,
    DocumentRef,
    ENContent,
    EvidenceFile,
    JPContent,
)
from sve_carddb.domains.products.identity_models import SourceBlock
from sve_carddb.domains.text_observations.presence import EffectPresence, PresenceResult
from sve_carddb.ingest.archive.source_archive import Descriptor
from sve_carddb.parse.pages import extract_en, extract_jp

if TYPE_CHECKING:
    from pathlib import Path


def _document(
    content: JPContent | ENContent, cards: CardFile
) -> JPDocument | ENDocument:
    record = content.record
    values = {
        name: getattr(record, name)
        for name in type(record).model_fields
        if name != "products_ref"
    }
    values["products"] = list(cards.products[record.products_ref])
    for name in ("faces", "notes", "qa", "related_cards"):
        values[name] = list(values[name])
    if content.region == "jp":
        return JPDocument.model_construct(
            region="jp", record=construct(extract_jp.CardRecord, values)
        )
    return ENDocument.model_construct(
        region="en", record=construct(extract_en.CardRecord, values)
    )


def _observation(
    ref: DocumentRef, document: JPDocument | ENDocument, evidence: EvidenceFile
) -> ParsedObservation:
    source = ref.source
    saved = evidence.observations[source.id]
    raw, shared, refs = saved.raw, evidence.shared, saved.layout
    region = document.region
    descriptor = Descriptor.model_construct(
        id=source.id,
        # This derives a runtime provenance field; it does not verify content.
        source_key=digest(
            canonical({"provider": region, "kind": "card", "url": source.url})
        ),
        provider=region,
        kind="card",
        url=source.url,
        raw_sha256=raw.raw_sha256,
        raw_bytes=raw.raw_bytes,
        first_receipt_id=raw.first_receipt_id,
    )
    presence = []
    for item in refs.presence:
        template = shared.presence[item.template]
        parser = shared.parsers[template.parser]
        result = PresenceResult.model_construct(
            recipe=parser.recipe,
            parser_version=parser.parser_version,
            template_id=parser.template_id,
            source_version_id=source.id,
            source_index=template.source_index,
            container_locator=shared.locators[template.container_locator],
            state=template.state,
            reason_code=template.reason_code,
        )
        presence.append(
            EffectPresence.model_construct(result=result, result_hash=item.result_hash)
        )
    products = tuple(
        ProductEvidence.model_construct(
            ordinal=block.ordinal,
            name=block.name,
            date_raw=block.date_raw,
            href_raw=block.href_raw,
            resolved_urls=block.resolved_urls,
            diagnostics=block.diagnostics,
            matches=tuple(
                SourceBlock.model_construct(
                    kind="source_block",
                    source_version_id=source.id,
                    product_block_ordinal=match.product_block_ordinal,
                )
                if match.kind == "source_block"
                else match
                for match in block.matches
            ),
        )
        for block in shared.products[refs.products]
    )
    recipe = shared.recipes[saved.recipe]
    return ParsedObservation.model_construct(
        recipe=recipe.recipe,
        producer_revision=recipe.producer_revision,
        evidence=Evidence.model_construct(
            descriptor=descriptor,
            descriptor_sha256=raw.descriptor_sha256,
            fetched_at=source.fetched_at,
            etag=raw.etag,
            last_modified=raw.last_modified,
        ),
        document=document,
        layout=LayoutEvidence.model_construct(
            presence=tuple(presence),
            whitespace=refs.whitespace,
            products=products,
            qa=shared.qa[refs.qa],
            related=shared.related[refs.related],
            errata_urls=shared.errata_urls[refs.errata_urls],
        ),
        content_hash=saved.content_hash,
    )


def _pair(cards: CardFile, evidence: EvidenceFile) -> FilePair:
    members = []
    for card in cards.cards:
        documents = {
            key: _document(value, cards) for key, value in card.documents.items()
        }
        members.append(
            Card.model_construct(
                card_no=card.card_no,
                observations=tuple(
                    _observation(ref, documents[ref.document_ref], evidence)
                    for ref in card.observations
                ),
            )
        )
    return FilePair.model_construct(
        region=cards.region,
        family=cards.family,
        range=cards.range,
        cards=tuple(members),
    )


def read(root: Path) -> Catalog:
    """Load only listed members, without schema, hash, closure or history validation."""
    index = decode(Index, member(root, "index.json").read_bytes())
    files = tuple(
        _pair(
            decode(CardFile, member(root, pin.path).read_bytes()),
            decode(EvidenceFile, member(root, pin.evidence_path).read_bytes()),
        )
        for pin in index.files
    )
    batches = []
    for pin in index.batches:
        parts = [
            decode(BatchFile, member(root, pin.path(i)).read_bytes())
            for i in range(len(pin.parts))
        ]
        batches.append(
            BatchSelection.model_construct(
                id=pin.id,
                region=parts[0].region,
                versions={
                    version: current
                    for part in parts
                    for version, current in part.versions.items()
                },
            )
        )
    return Catalog(index, files, tuple(batches))
