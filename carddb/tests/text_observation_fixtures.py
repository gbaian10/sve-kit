"""Synthetic sealed observations with independently supplied current-field bindings."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.build_inputs import BuildContext
from sve_carddb.products import Language, load_products, populate_product_preview
from sve_carddb.registry.build import build
from sve_carddb.registry.preview import plan_preview
from sve_carddb.registry.storage import plan_files, write_files
from sve_carddb.snapshot.values import canonical
from sve_carddb.text_observations import (
    Binding,
    FaceContent,
    TextCard,
    Vocabulary,
    plan_text_observations,
    text_configuration,
)

from .build_input_fixtures import frozen_provider
from .product_fixtures import envelope, family, install
from .registry_preview_fixtures import REVISION

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.products.loader import ProductSnapshot
    from sve_carddb.registry.inputs import Card
    from sve_carddb.registry.preview import PreviewPlan
    from sve_carddb.registry.records import Region
    from sve_carddb.registry.review import Inputs
    from sve_carddb.text_observations import TextPlan

LANGUAGES = (
    Language(code="ja", fallback_order=(), display_name="Japanese"),
    Language(code="en", fallback_order=(), display_name="English"),
)


@dataclass
class MemoryTexts:
    cards: dict[tuple[Region, str], TextCard]

    def card(self, region: Region, card_no: str) -> TextCard | None:
        return self.cards.get((region, card_no))


@dataclass
class Case:
    root: Path
    store: Path
    identity: PreviewPlan
    provider: MemoryTexts
    plan: TextPlan
    catalog: ProductSnapshot
    vocabulary: Vocabulary

    def context(self) -> BuildContext:
        return BuildContext.from_inputs(
            REVISION,
            {"synthetic.lock": b"synthetic dependencies"},
            text_configuration(self.plan, self.vocabulary, ()),
        )

    def stage(self, db: Database) -> None:
        populate_product_preview(
            db,
            self.catalog,
            self.identity,
            authored_revision=REVISION,
            build=self.context(),
            languages=LANGUAGES,
            stores={"test-store": self.store},
        )


def make_case(
    root: Path, inputs: Inputs, *, regions: tuple[Region, ...] = ("jp", "en")
) -> Case:
    write_files(plan_files(root, build(inputs, {})))
    for identifier in ("BP02", "PR", "GF01"):
        install(
            root,
            f"products/family/{identifier}/001.yaml",
            envelope([family(identifier)]),
        )
    evidence, store = frozen_provider(inputs, root.parent / (root.name + "-archive"))
    identity = plan_preview(root, evidence, regions=regions)
    cards: dict[tuple[Region, str], TextCard] = {}
    collections: tuple[tuple[Region, dict[str, Card]], ...] = (
        ("jp", inputs.jp),
        ("en", inputs.en),
    )
    for region, collection in collections:
        for number, original in collection.items():
            proof = evidence.cards[region, number]
            faces = tuple(
                FaceContent(
                    name=face.name,
                    effect=face.text,
                    sections=tuple(face.sections),
                    class_raw=face.card_class if region == "jp" else face.info["Class"],
                    type_raw=face.card_type
                    if region == "jp"
                    else face.info["Card Type"],
                    stats=(face.cost, face.power, face.hp)
                    if region == "jp"
                    else (face.stats["cost"], face.stats["power"], face.stats["hp"]),
                    traits=tuple(face.traits)
                    if region == "jp"
                    else tuple(face.info["Trait"].split(" / ")),
                    flavor=face.speech,
                )
                for face in original.faces
            )
            cards[region, number] = TextCard(
                source=proof.source.model_copy(
                    update={"parser_version": "synthetic-text-v1"}
                ),
                observation=proof.observation,
                faces=faces,
            )
    provider = MemoryTexts(cards)
    plan = plan_text_observations(identity, provider)
    vocabulary = Vocabulary(
        bindings=(
            Binding(region="jp", kind="type", raw="フォロワー", code="follower"),
            Binding(region="en", kind="type", raw="Follower", code="follower"),
            Binding(region="jp", kind="class", raw="ウィッチ", code="runecraft"),
            Binding(region="en", kind="class", raw="Runecraft", code="runecraft"),
            Binding(region="en", kind="trait", raw="Mage", code="mage"),
        )
    )
    assert canonical(vocabulary.model_dump(mode="json"))
    return Case(
        root,
        store,
        identity,
        provider,
        plan,
        load_products(root, registry=identity.snapshot),
        vocabulary,
    )
