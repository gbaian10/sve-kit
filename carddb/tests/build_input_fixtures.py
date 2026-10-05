"""Synthetic sealed raw inputs shared by identity and product evidence tests."""

from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.manifest import Kind, Region
from sve_carddb.products.evidence import resolve_evidence
from sve_carddb.products.importer import product_source_uses
from sve_carddb.registry.inputs import Card
from sve_carddb.registry.preview.evidence import CardEvidence, FaceEvidence
from sve_carddb.registry.storage import read_yaml
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources import official_en
from sve_carddb.sources.official_jp import card_url

from .identity_evidence_fixtures import MemoryEvidence
from .product_fixtures import first_record, install, obj
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_inputs import SourceUse
    from sve_carddb.products.loader import ProductSnapshot
    from sve_carddb.registry.preview import PreviewPlan
    from sve_carddb.registry.records import Region as CardRegion
    from sve_carddb.registry.review import Inputs

NAME = "products/family/BP02/001.yaml"


def frozen_provider(inputs: Inputs, temporary: Path) -> tuple[MemoryEvidence, Path]:
    store = _store(temporary)
    cards = {("jp", item.number): item for item in inputs.jp.values()}
    cards.update({("en", item.number): item for item in inputs.en.values()})
    for (region, number), card in cards.items():
        raw = b"<html>" + card.model_dump_json().encode() + b"</html>"
        url = card_url(number) if region == "jp" else official_en.card_url(number)
        _put(
            store,
            replace(
                _resource(url, f"raw/{region}-{number}.html", raw, Kind.CARD),
                region=Region.JP if region == "jp" else Region.EN,
            ),
            raw,
        )
    sealed = seal_batch(store)
    sources = FrozenSources(store.root, store.store_id, sealed.batch_id)
    found: dict[tuple[CardRegion, str], CardEvidence] = {}
    for current in sources.inventory.current:
        source, raw, descriptor = sources.read(
            current.source_version_id, parser_version="synthetic-json-html-v1"
        )
        card = Card.model_validate_json(raw[len(b"<html>") : -len(b"</html>")])
        observed_region: CardRegion = "jp" if descriptor.provider == "jp" else "en"
        found[observed_region, card.number] = CardEvidence.from_card(
            source,
            observed_region,
            card,
            tuple(FaceEvidence("LG", None) for _ in card.faces),
        )
    return MemoryEvidence(found, frozenset({inputs.jp_hash})), store.root


def reference_identity_source(root: Path, provider: MemoryEvidence) -> None:
    source = provider.cards["jp", "BP02-071"].source
    raw = obj(read_yaml(root / NAME))
    first_record(raw)["evidence"] = [
        {
            "batch_id": source.archive.batch_id,
            "source_version_id": source.id,
            "locator": "product block 0",
            "role": "family review",
        }
    ]
    install(root, NAME, raw, resign=True)


def expected_uses(
    catalog: ProductSnapshot, plan: PreviewPlan, store: Path
) -> tuple[SourceUse, ...]:
    references = tuple(
        ref for record in catalog.records.values() for ref in record.evidence
    )
    evidence = resolve_evidence(references, {"test-store": store})
    return (*product_source_uses(catalog, evidence), *plan.source_uses())
