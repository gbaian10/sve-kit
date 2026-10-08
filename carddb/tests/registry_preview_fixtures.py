"""Synthetic registry evidence and caller-supplied product parents."""

from typing import TYPE_CHECKING

from sve_carddb.core.provenance import ArchivePin, BuildContext, Source
from sve_carddb.registry.inputs import digest
from sve_carddb.registry.preview.evidence import CardEvidence, FaceEvidence
from sve_carddb.registry.records import CardData, PrintingData, Region

from .build_db_fixtures import rows
from .identity_evidence_fixtures import MemoryEvidence

if TYPE_CHECKING:
    from sve_carddb.build.database import Database
    from sve_carddb.registry.inputs import Card
    from sve_carddb.registry.preview import PreviewPlan
    from sve_carddb.registry.review import Inputs

REVISION = "a" * 40
BUILD = BuildContext.from_inputs(REVISION, {"synthetic": True})


def observed(card: Card, region: Region) -> CardEvidence:
    """Use real observation hashing on synthetic card data."""
    raw_hash = digest(card.model_dump(mode="json"))
    source = Source(
        id="src:v1:"
        + digest({"region": region, "number": card.number}).removeprefix("sha256:"),
        url=f"https://example.invalid/{region}/{card.number}",
        raw_locator="synthetic:raw/" + raw_hash.removeprefix("sha256:"),
        sha256=raw_hash,
        fetched_at="2026-09-29T00:00:00Z",
        parser_version="synthetic-v1",
        archive=ArchivePin(
            store_id="synthetic",
            batch_id="sha256:" + "3" * 64,
            descriptor_sha256="sha256:" + "4" * 64,
            first_receipt_id="sha256:" + "5" * 64,
        ),
    )
    return CardEvidence.from_card(
        source,
        region,
        card,
        tuple(
            FaceEvidence("LG", f"Synthetic artist {n}") for n in range(len(card.faces))
        ),
    )


def evidence(inputs: Inputs, *, en: bool = True) -> MemoryEvidence:
    """Pin independent synthetic sources and the reviewed full-input hash."""
    collections: tuple[tuple[Region, dict[str, Card]], ...] = (
        ("jp", inputs.jp),
        ("en", inputs.en),
    )
    cards = {
        (region, card.number): observed(card, region)
        for region, collection in collections
        if region == "jp" or en
        for card in collection.values()
    }
    return MemoryEvidence(cards, frozenset({inputs.jp_hash}))


def parents(db: Database, plan: PreviewPlan) -> None:
    """Emulate separately verified product input, never production placeholders."""
    fixtures = rows()
    with db.transaction():
        for name in ("language", "text_unit"):
            db.insert(name, fixtures[name])
        required = {
            r.data.home_set_id
            for kind in ("card", "printing")
            for r in plan.included(kind)
            if isinstance(r.data, (CardData, PrintingData))
        }
        for index, family in enumerate(sorted(required)):
            db.insert(
                "product_family",
                fixtures["product_family"]
                | {"id": family, "code": f"test{index}", "public_code": f"TEST{index}"},
            )
