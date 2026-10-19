"""Join human-readable cards to sidecar evidence without changing their semantics."""

from dataclasses import asdict, dataclass, field
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, digest, object_value
from sve_carddb.domains.parsed_cards.models import (
    Card,
    ENDocument,
    Evidence,
    FilePair,
    JPDocument,
    LayoutEvidence,
    ParsedObservation,
    ProductEvidence,
)
from sve_carddb.domains.parsed_cards.wire import (
    CardDocuments,
    CardFile,
    Document,
    DocumentRef,
    EvidenceFile,
    LayoutRefs,
    ObservationEvidence,
    PresenceRef,
    PresenceTemplate,
    ProductTemplate,
    RawEvidence,
    Recipe,
    Shared,
    SourceRef,
)
from sve_carddb.domains.text_observations.presence import EffectPresence, PresenceResult
from sve_carddb.ingest.archive.source_archive import Descriptor

if TYPE_CHECKING:
    from pydantic import JsonValue


@dataclass
class _Pool:
    values: dict[str, dict[str, JsonValue]] = field(default_factory=dict)

    def put(self, category: str, value: JsonValue) -> str:
        key = category + "-" + digest(canonical(value))[7:23]
        group = self.values.setdefault(category, {})
        if key in group and group[key] != value:
            raise ValueError("Parsed evidence key collision")
        group[key] = value
        return key

    def shared(self) -> Shared:
        values: dict[str, JsonValue] = {
            name: dict(sorted(self.values.get(name, {}).items()))
            for name in Shared.model_fields
        }
        return Shared.model_validate_json(canonical(values))


def _observation(value: ParsedObservation, pool: _Pool) -> ObservationEvidence:
    descriptor = value.evidence.descriptor
    recipe = Recipe(recipe=value.recipe, producer_revision=value.producer_revision)
    layout = value.layout.model_dump(mode="json")
    return ObservationEvidence(
        recipe=pool.put("recipes", recipe.model_dump(mode="json")),
        raw=RawEvidence(
            raw_sha256=descriptor.raw_sha256,
            raw_bytes=descriptor.raw_bytes,
            first_receipt_id=descriptor.first_receipt_id,
            descriptor_sha256=value.evidence.descriptor_sha256,
            etag=value.evidence.etag,
            last_modified=value.evidence.last_modified,
        ),
        layout=LayoutRefs(
            presence=tuple(
                PresenceRef(
                    template=_template(p.result, pool),
                    result_hash=p.result_hash,
                )
                for p in value.layout.presence
            ),
            whitespace=value.layout.whitespace,
            products=pool.put("products", _products(value)),
            qa=pool.put("qa", layout["qa"]),
            related=pool.put("related", layout["related"]),
            errata_urls=pool.put("errata", layout["errata_urls"]),
        ),
        content_hash=value.content_hash,
    )


def _template(result: PresenceResult, pool: _Pool) -> str:
    value = result.model_dump(mode="json", exclude={"source_version_id"})
    parser = {
        key: value.pop(key) for key in ("recipe", "parser_version", "template_id")
    }
    value["parser"] = pool.put("parsers", parser)
    value["container_locator"] = pool.put("locators", value["container_locator"])
    template = PresenceTemplate.model_validate_json(canonical(value))
    return pool.put("presence", template.model_dump(mode="json"))


def separate(pair: FilePair) -> tuple[CardFile, EvidenceFile]:
    """Keep official records readable and put repeated build evidence in shared tables."""
    pool = _Pool()
    versions = [
        value.evidence.descriptor.id
        for card in pair.cards
        for value in card.observations
    ]
    if len(versions) != len(set(versions)):
        raise ValueError("Duplicate parsed source version")
    observations = {
        value.evidence.descriptor.id: _observation(value, pool)
        for card in pair.cards
        for value in card.observations
    }
    cards = _cards(pair)
    # Category names are part of the wire contract; errata is a short reference prefix.
    pool.values["errata_urls"] = pool.values.pop("errata", {})
    evidence = EvidenceFile(
        region=pair.region,
        family=pair.family,
        range=pair.range,
        shared=pool.shared(),
        observations=dict(sorted(observations.items())),
    )
    return cards, evidence


def _presence(
    template: PresenceTemplate, reference: PresenceRef, version: str, shared: Shared
) -> EffectPresence:
    value = template.model_dump(mode="json", exclude={"parser"})
    value["container_locator"] = shared.locators[template.container_locator]
    result = PresenceResult.model_validate_json(
        canonical(
            {
                **value,
                **shared.parsers[template.parser].model_dump(mode="json"),
                "source_version_id": version,
            }
        )
    )
    return EffectPresence(result=result, result_hash=reference.result_hash)


def _restore(
    document: Document, evidence: ObservationEvidence, shared: Shared, cards: CardFile
) -> ParsedObservation:
    source, raw = document.source, evidence.raw
    region = document.document.region
    descriptor = Descriptor(
        id=source.id,
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
    recipe = shared.recipes[evidence.recipe]
    refs = evidence.layout
    return ParsedObservation(
        recipe=recipe.recipe,
        producer_revision=recipe.producer_revision,
        evidence=Evidence(
            descriptor=descriptor,
            descriptor_sha256=raw.descriptor_sha256,
            fetched_at=source.fetched_at,
            etag=raw.etag,
            last_modified=raw.last_modified,
        ),
        document=_expanded(document, cards),
        layout=LayoutEvidence(
            presence=tuple(
                _presence(shared.presence[p.template], p, source.id, shared)
                for p in refs.presence
            ),
            whitespace=refs.whitespace,
            products=_restore_products(shared.products[refs.products], source.id),
            qa=shared.qa[refs.qa],
            related=shared.related[refs.related],
            errata_urls=shared.errata_urls[refs.errata_urls],
        ),
        content_hash=evidence.content_hash,
    )


def combine(cards: CardFile, evidence: EvidenceFile) -> FilePair:
    """Validate every join and reconstruct all original fields and content hashes."""
    if (cards.region, cards.family, cards.range) != (
        evidence.region,
        evidence.family,
        evidence.range,
    ):
        raise ValueError("Parsed evidence belongs to another card file")
    versions = [d.source.id for c in cards.cards for d in c.observations]
    if len(versions) != len(set(versions)) or set(versions) != set(
        evidence.observations
    ):
        raise ValueError("Parsed evidence/card source closure differs")
    try:
        pair = FilePair(
            region=cards.region,
            family=cards.family,
            range=cards.range,
            cards=tuple(
                Card(
                    card_no=c.card_no,
                    observations=tuple(
                        _restore(
                            Document(
                                source=d.source, document=c.documents[d.document_ref]
                            ),
                            evidence.observations[d.source.id],
                            evidence.shared,
                            cards,
                        )
                        for d in c.observations
                    ),
                )
                for c in cards.cards
            ),
        )
    except KeyError:
        raise ValueError("Parsed evidence has a dangling shared reference") from None
    if separate(pair) != (cards, evidence):
        raise ValueError("Parsed evidence is not canonically interned")
    return pair


def _products(value: ParsedObservation) -> list[JsonValue]:
    blocks: list[JsonValue] = []
    for block in value.layout.products:
        data = block.model_dump(mode="json")
        for match in data["matches"]:
            if (
                match["kind"] == "source_block"
                and match.pop("source_version_id") != value.evidence.descriptor.id
            ):
                raise ValueError("Parsed product match belongs to another source")
        blocks.append(data)
    return blocks


def _restore_products(
    blocks: tuple[ProductTemplate, ...], version: str
) -> tuple[ProductEvidence, ...]:
    result = []
    for block in blocks:
        data = block.model_dump(mode="json")
        for match in data["matches"]:
            if match["kind"] == "source_block":
                match["source_version_id"] = version
        result.append(ProductEvidence.model_validate_json(canonical(data)))
    return tuple(result)


def _readable(value: ParsedObservation, pool: _Pool) -> Document:
    data = value.document.model_dump(mode="json")
    record = object_value(data["record"])
    record["products_ref"] = pool.put("products", record.pop("products"))
    return Document.model_validate_json(
        canonical(
            {
                "source": SourceRef(
                    id=value.evidence.descriptor.id,
                    url=value.evidence.descriptor.url,
                    fetched_at=value.evidence.fetched_at,
                ).model_dump(mode="json"),
                "document": {"region": value.document.region, "record": record},
            }
        )
    )


def _cards(pair: FilePair) -> CardFile:
    pool = _Pool()
    cards = []
    for card in pair.cards:
        documents = _Pool()
        observations = []
        for value in card.observations:
            readable = _readable(value, pool)
            key = documents.put("documents", readable.document.model_dump(mode="json"))
            observations.append(DocumentRef(source=readable.source, document_ref=key))
        cards.append(
            CardDocuments.model_validate_json(
                canonical(
                    {
                        "card_no": card.card_no,
                        "documents": dict(
                            sorted(documents.values["documents"].items())
                        ),
                        "observations": [
                            o.model_dump(mode="json") for o in observations
                        ],
                    }
                )
            )
        )
    return CardFile.model_validate_json(
        canonical(
            {
                "region": pair.region,
                "family": pair.family,
                "range": pair.range,
                "products": dict(sorted(pool.values.get("products", {}).items())),
                "cards": [card.model_dump(mode="json") for card in cards],
            }
        )
    )


def _expanded(document: Document, cards: CardFile) -> JPDocument | ENDocument:
    data = document.document.model_dump(mode="json")
    record = object_value(data["record"])
    record.pop("products_ref")
    record["products"] = [
        asdict(p) for p in cards.products[document.document.record.products_ref]
    ]
    data["record"] = record
    model = JPDocument if document.document.region == "jp" else ENDocument
    return model.model_validate_json(canonical(data))
