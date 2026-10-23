"""Real pinned JP inputs through production identity and current-text writers."""

from dataclasses import dataclass
from pathlib import Path

from sve_carddb.build import create_database
from sve_carddb.build.t1 import MINIMUM_CAPABILITIES, compile_build
from sve_carddb.core.authored import read as read_authored
from sve_carddb.core.json import canonical, object_value, parse, string
from sve_carddb.core.provenance import BuildContext
from sve_carddb.domains.catalog.adoption_loader import load_adoptions
from sve_carddb.domains.catalog.rules_names import populate_rules_names
from sve_carddb.domains.parsed_cards.reader import read
from sve_carddb.domains.parsed_cards.sources import ParsedBatches
from sve_carddb.domains.products import load_products, populate_product_preview
from sve_carddb.domains.registry.preview import FrozenJP, plan_preview
from sve_carddb.domains.registry.records import PrintingData
from sve_carddb.domains.text_observations import (
    FrozenTexts,
    TextObservations,
    Vocabulary,
)
from sve_carddb.domains.text_observations.closure import close_preview
from sve_carddb.domains.translations.four_layer_authored import FrameRecord, Shard
from sve_carddb.domains.translations.glossary.records import TermRecord
from sve_carddb.domains.translations.inputs import Snapshot, validate_snapshot
from sve_carddb.domains.translations.recognition.rules import Rules, load
from sve_carddb.domains.translations.sources import Sources

from .database_template import DatabaseTemplate

REPOSITORY = Path(__file__).resolve().parents[3]
REVISION = "8ca79f0935dd97340991c057cb1b226fe745da50"
PARSER = "author-current-fixture-v1"
CARD_NUMBERS = frozenset(
    {
        "BP01-009",
        "BP01-034",
        "BP01-092",
    }
)
EVOLVE_FRAME = "frame:4e38fd488749ed51f1cf501759988e443549000c47bca7aba285a2f6009b902d"


@dataclass(frozen=True)
class CurrentSourceFixture:
    template: DatabaseTemplate
    sources: Sources
    snapshot: Snapshot
    vocabulary: Vocabulary
    rules: Rules
    jp_batch_id: str


def entry_snapshot() -> Snapshot:
    """Read only the real frame and term shards needed by the adopted entries.

    The adopted entry frame and Storm term retain their existing definitions.
    This does not verify the full registry's private digital concept sources.
    """
    authored = REPOSITORY / "authored"
    records = tuple(
        record
        for name in (
            "translations/glossary/concepts/004.yaml",
            "translations/templates/definitions/094.yaml",
        )
        for record in Shard.model_validate_json(
            read_authored(authored / name, root=authored)[1]
        ).records
    )
    selected = tuple(
        record
        for record in records
        if isinstance(record, FrameRecord) and record.data.id == EVOLVE_FRAME
    )
    assert len(selected) == 1
    storm = tuple(
        record
        for record in records
        if isinstance(record, TermRecord) and record.data.id == "term:keyword.storm"
    )
    assert len(storm) == 1
    content = canonical(
        {
            "format": 3,
            "kind": "translation_shard",
            "records": [
                selected[0].model_dump(mode="json", exclude_computed_fields=True)
            ],
        }
    )
    glossary = canonical(
        {
            "format": 3,
            "kind": "translation_shard",
            "records": [storm[0].model_dump(mode="json", exclude_computed_fields=True)],
        }
    )
    glossary_file = ("translations/glossary/concepts/004.yaml", glossary, glossary)
    snapshot = Snapshot(
        (glossary_file,),
        (
            glossary_file,
            ("translations/templates/definitions/094.yaml", content, content),
        ),
    )
    validate_snapshot(snapshot)
    return snapshot


def current_source_fixture(store: Path) -> CurrentSourceFixture:
    """Build from public parsed inputs; the store is only an explicit provenance root."""
    from sve_carddb.domains.catalog.loader import prepare  # ruff: ignore[import-outside-top-level] -- initialize the production text interner before the catalog importer

    authored = REPOSITORY / "authored"
    batch_id = string(
        object_value(parse((REPOSITORY / "tests/engine/card-input.json").read_bytes()))[
            "batch_id"
        ]
    )
    batches = ParsedBatches(read(REPOSITORY / "card-data"))
    frozen = batches.batch(store, "public-fixture", batch_id)
    identity = plan_preview(
        authored,
        FrozenJP(
            store, "public-fixture", batch_id, parser_version=PARSER, sources=frozen
        ),
        regions=("jp",),
    )
    printings = tuple(
        record.data
        for record in identity.included("printing")
        if isinstance(record.data, PrintingData)
    )
    cards = {
        printing.card_id for printing in printings if printing.card_no in CARD_NUMBERS
    }
    selected = close_preview(
        identity,
        frozenset(
            (printing.card_id, printing.region)
            for printing in printings
            if printing.card_id not in cards
        ),
    )
    observations = TextObservations(
        selected,
        FrozenTexts(
            store,
            "public-fixture",
            batch_id,
            region="jp",
            parser_version=PARSER,
            sources=frozen,
        ),
    )
    stores = {"public-fixture": store}
    prepared = prepare(
        (load_adoptions(authored, entry="catalog/adoptions"),),
        REPOSITORY,
        BuildContext.from_inputs(REVISION, {}),
        stores,
        identity.snapshot,
        batches=batches,
    )
    vocabulary = prepared.projection.vocabulary
    build = BuildContext.from_inputs(
        REVISION, observations.configuration(vocabulary, ())
    )
    schema = compile_build((*MINIMUM_CAPABILITIES, "en", "translation_evidence"))
    with create_database(schema) as db:
        with db.transaction():
            populate_product_preview(
                db,
                load_products(authored, registry=identity.snapshot),
                identity,
                authored_revision=REVISION,
                build=build,
                languages=prepared.projection.catalog.languages,
                stores=stores,
                batches=batches,
            )
            observations.populate(db, build=build, vocabulary=vocabulary, published=())
            populate_rules_names(db)
        template = DatabaseTemplate(schema, db._connection.serialize())
    return CurrentSourceFixture(
        template,
        Sources(stores, REPOSITORY, build, identity.snapshot, batches=batches),
        entry_snapshot(),
        vocabulary,
        load(REPOSITORY),
        batch_id,
    )
