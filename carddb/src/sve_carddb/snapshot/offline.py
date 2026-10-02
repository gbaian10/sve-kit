"""Compose regional card-page supplements without granting formal release coverage."""

from dataclasses import replace
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Pydantic resolves recipe paths at runtime
from typing import TYPE_CHECKING

from pydantic import JsonValue, model_validator

from sve_carddb.build_bundle import publish_bundle
from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_minimum
from sve_carddb.build_inputs import BuildContext, Revision, input_record, uses_sorted
from sve_carddb.card_extras import (
    FrozenCardExtras,
    applicable_reskin_regions,
    plan_card_extras,
    populate_card_extras,
    require_card_extras_ready,
)
from sve_carddb.catalog.adoption_sources import AdoptionSources, PinnedRepository
from sve_carddb.products import (
    FrozenProducts,
    load_product_identities,
    load_products,
    plan_official_products,
)
from sve_carddb.products.models import Date  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves constrained fields at runtime
from sve_carddb.registry.preview import FrozenEN, FrozenJP, FrozenRegions, plan_preview
from sve_carddb.registry.records import (
    Hash,
    Instant,
    PrintingData,
    RecordData,
    Region,
    Text,
)
from sve_carddb.snapshot.contract import validate
from sve_carddb.snapshot.export import Batch, Ownership
from sve_carddb.snapshot.preview.build import Built
from sve_carddb.snapshot.project import Decisions, Settings, project
from sve_carddb.snapshot.values import array, digest
from sve_carddb.source_corrections import FrozenImages
from sve_carddb.text_observations import (
    FrozenTexts,
    RegionalTexts,
    plan_text_observations,
    populate_text_preview,
    text_configuration,
)
from sve_carddb.text_observations.composition import text_preview_uses
from sve_carddb.text_observations.wording import printing_observed_texts, wording_views

if TYPE_CHECKING:
    from sve_carddb.build_db import CompiledSchema, Database
    from sve_carddb.build_inputs import InputRecord, Source, SourceUse
    from sve_carddb.card_extras import ErrataPage, ExtrasPlan
    from sve_carddb.catalog.adoption_importer import AdoptionInputs
    from sve_carddb.catalog.projection import CatalogProjection
    from sve_carddb.products import ProductIdentities
    from sve_carddb.registry.records import CorrectionEvidence
    from sve_carddb.snapshot.project import Projection


class RegionalInput(RecordData):
    region: Region
    card_batch: Hash
    image_batch: Hash
    parser_version: Text


class Inputs(RecordData):
    repo: Path
    archive: Path
    store_id: Text
    sources: tuple[RegionalInput, ...]
    revision: Revision
    as_of: Date
    data_version: Text
    published_at: Instant
    feedback_url: Text
    grammar_version: Text
    normalizer_version: Text

    @model_validator(mode="after")
    def regional_scope(self) -> Inputs:
        """Require both launch regions and their languages, without implicit defaults."""
        if tuple(pin.region for pin in self.sources) != ("en", "jp"):
            raise ValueError("Offline launch inputs require sorted EN and JP pins")
        validate("DataVersion", self.data_version)
        if not self.data_version.startswith("preview-"):
            raise ValueError("Offline composition requires a preview- data version")
        return self

    def batch(self) -> Batch:
        """Keep the launch scope explicit rather than deriving it from loaded rows."""
        return Batch(self.data_version, self.published_at, ("en", "jp"))


class RegionalImages:
    def __init__(self, inputs: Inputs) -> None:
        self.providers = {
            pin.region: FrozenImages(inputs.archive, inputs.store_id, pin.image_batch)
            for pin in inputs.sources
        }

    def image(self, evidence: CorrectionEvidence) -> Source:
        """Dispatch correction evidence only to its explicitly pinned regional batch."""
        return self.providers[evidence.region].image(evidence)


def require_offline_coverage(projection: Projection, *, errata: bool) -> None:
    """Card pages prove observations, not index completeness or rules coverage."""
    if (
        projection.metadata["source_windows"]
        or projection.metadata["restriction_coverage"]
    ):
        raise ValueError("Uncovered sources must remain empty windows / unknown")
    disabled = ("cr_version", "restriction") + (() if errata else ("errata",))
    if any(projection.tables[table] for table in disabled):
        raise ValueError("Unrequested ancillary sources cannot become public facts")


def _dependencies(repo: Path, identities: ProductIdentities) -> dict[str, bytes]:
    dependencies = {
        path.relative_to(repo).as_posix(): path.read_bytes()
        for path in (repo / "carddb/src/sve_carddb").rglob("*.py")
        if path.name != "_version.py"
    } | identities.dependencies()
    dependencies["carddb/uv.lock"] = (repo / "carddb/uv.lock").read_bytes()
    return dependencies


def _derive_adoptions(
    inputs: AdoptionInputs,
    schema: CompiledSchema,
    context: BuildContext,
    stores: dict[str, Path],
) -> CatalogProjection:
    """Derive only from checked receipts before any candidate text is written."""
    from sve_carddb.catalog.adoption_importer import derive_catalog  # ruff: ignore[import-outside-top-level] -- load after the text interner to avoid the catalog/text package import cycle

    with create_database(schema) as probe:
        derived = derive_catalog(probe, inputs, build=context, stores=stores)
    if not {"en", "ja"} <= {language.code for language in derived.catalog.languages}:
        raise ValueError("Offline launch requires adopted EN and JA languages")
    return derived


def _adoption_uses(
    inputs: AdoptionInputs, stores: dict[str, Path]
) -> tuple[SourceUse, ...]:
    """Replay the complete receipt evidence independently of database insertion."""
    sources = AdoptionSources(stores, PinnedRepository(inputs.repository))
    for snapshot in inputs.load():
        for shard in snapshot.shards:
            envelope = shard.envelope()
            for record in envelope.records:
                sources.verify(record, envelope.review_context)
    return uses_sorted(sources.uses)


def _source_gaps(db: Database, extras: ExtrasPlan) -> list[JsonValue]:
    printing_index = {
        (row.values["region"], row.values["card_no"]): str(row.values["id"])
        for row in db.rows("printing")
    }
    face_index: dict[str, list[str]] = {}
    for row in db.rows("printing_face"):
        face_index.setdefault(str(row.values["printing_id"]), []).append(
            str(row.values["face_id"])
        )
    source_gaps: list[JsonValue] = []
    for gap in extras.gaps:
        printing_id = printing_index.get((gap.region, gap.card_no))
        gap_value = gap.report()
        gap_value["printing_id"] = printing_id
        gap_value["face_ids"] = list[JsonValue](
            sorted(face_index.get(printing_id or "", []))
        )
        source_gaps.append(gap_value)
    return source_gaps


def build(  # ruff: ignore[too-many-locals] -- one composition binds verified domain plans in a single transaction
    inputs: Inputs,
    *,
    errata: tuple[ErrataPage, ...] = (),
    bundle_dir: Path | None = None,
) -> Built:
    """Build both regions from sealed sources, retaining every diagnostic source use."""
    from sve_carddb.catalog.adoption_importer import AdoptionInputs, populate_adoptions  # ruff: ignore[import-outside-top-level] -- load after text modules initialize the shared interner

    if bundle_dir is not None:
        for protected in (inputs.repo, inputs.archive):
            output, source = bundle_dir.resolve(), protected.resolve()
            if output.is_relative_to(source) or source.is_relative_to(output):
                raise ValueError(
                    "Offline bundle must be disjoint from immutable inputs"
                )
    en, jp = inputs.sources
    identity = plan_preview(
        inputs.repo / "authored",
        FrozenRegions(
            en=FrozenEN(
                inputs.archive,
                inputs.store_id,
                en.card_batch,
                parser_version=en.parser_version,
            ),
            jp=FrozenJP(
                inputs.archive,
                inputs.store_id,
                jp.card_batch,
                parser_version=jp.parser_version,
            ),
        ),
        regions=("en", "jp"),
    )
    texts = plan_text_observations(
        identity,
        RegionalTexts(
            {
                pin.region: FrozenTexts(
                    inputs.archive,
                    inputs.store_id,
                    pin.card_batch,
                    region=pin.region,
                    parser_version=pin.parser_version,
                )
                for pin in inputs.sources
            }
        ),
        images=RegionalImages(inputs),
    )
    publication = texts.publication_identity()
    printings = frozenset(
        record.data.id
        for record in publication.included("printing")
        if isinstance(record.data, PrintingData)
    )
    catalog = load_products(inputs.repo / "authored", registry=identity.snapshot)
    stores = {inputs.store_id: inputs.archive}
    identities = load_product_identities(
        inputs.repo / "authored",
        authored_revision=inputs.revision,
        catalog=catalog,
        stores=stores,
    )
    official = plan_official_products(
        identities,
        tuple(
            page
            for pin in inputs.sources
            for page in FrozenProducts(
                inputs.archive, inputs.store_id, pin.card_batch, region=pin.region
            ).pages()
        ),
        identity,
    )
    pages = tuple(
        page
        for pin in inputs.sources
        for page in FrozenCardExtras(
            inputs.archive, inputs.store_id, pin.card_batch, region=pin.region
        ).pages()
    )
    dependencies = _dependencies(inputs.repo, identities)
    adoptions = AdoptionInputs(
        inputs.repo / "authored", inputs.repo, inputs.revision, ("catalog-adoptions",)
    )
    configuration = adoptions.configuration() | {
        "product_identity": identities.configuration(),
        "offline_recipe": inputs.model_dump(mode="json", exclude={"repo", "archive"}),
        "selected_regions": ["en", "jp"],
        "published_history": "explicit-empty-no-releases",
    }
    context = BuildContext.from_inputs(inputs.revision, dependencies, configuration)
    schema = compile_minimum(include_en=True)
    derived = _derive_adoptions(adoptions, schema, context, stores)
    vocabulary = derived.vocabulary
    configuration |= text_configuration(texts, vocabulary, ())
    context = BuildContext.from_inputs(inputs.revision, dependencies, configuration)
    adoption_uses = _adoption_uses(adoptions, stores)
    with create_database(schema) as db:
        with db.transaction():
            parents = populate_text_preview(
                db,
                catalog,
                texts,
                authored_revision=inputs.revision,
                build=context,
                vocabulary=vocabulary,
                published=(),
                languages=derived.catalog.languages,
                stores=stores,
                official=official,
            )
            adopted = populate_adoptions(db, adoptions, build=context, stores=stores)
            extras = plan_card_extras(db, pages, errata=errata)
            # The parent record is private staging; only the complete context is emitted.
            context = BuildContext.from_inputs(
                inputs.revision,
                dependencies,
                configuration
                | {
                    "card_extras": extras.configuration(),
                },
            )
            added = populate_card_extras(db, extras, build=context)
            expected = uses_sorted(
                (
                    *text_preview_uses(catalog, texts, stores, official=official),
                    *extras.source_uses(),
                    *adoption_uses,
                )
            )
            record = input_record(context, (*parents.uses, *added.uses, *adopted.uses))
            record.verify(db, context, expected)
        restrictions = require_card_extras_ready(
            db,
            tuple(
                sorted(
                    {
                        (row.data.region, row.data.card_id)
                        for row in publication.included("printing")
                        if isinstance(row.data, PrintingData)
                    }
                )
            ),
        )
        decisions = replace(
            Decisions(),
            related_regions=applicable_reskin_regions(db, texts, vocabulary=vocabulary),
            supplemental_restrictions=restrictions,
        ).with_text_views(wording_views(db, texts), printing_observed_texts(db, texts))
        projection = project(
            db,
            regions=("en", "jp"),
            as_of=inputs.as_of,
            settings=Settings(
                inputs.feedback_url, inputs.grammar_version, inputs.normalizer_version
            ),
            decisions=decisions,
            publication_printings=printings,
        )
        require_offline_coverage(projection, errata=bool(errata))
        ownership = Ownership.from_database(db, projection)
        source_gaps = _source_gaps(db, extras)
    content = record.content()
    report: dict[str, JsonValue] = {
        "input_sha256": digest(content),
        "adopted_vocabulary": {
            "terms": len(vocabulary.terms),
            "bindings": len(vocabulary.bindings),
            "languages": [item.code for item in derived.catalog.languages],
        },
        "regions": ["en", "jp"],
        "counts": {table: len(rows) for table, rows in projection.tables.items()},
        "card_extras": extras.report(),
        "source_gaps": source_gaps,
        "regional_counts": {
            region: {
                "pages": sum(page.region == region for page in pages),
                "qa_blocks": sum(
                    len(page.qa) for page in pages if page.region == region
                ),
                "related_links": sum(
                    len(page.related) for page in pages if page.region == region
                ),
            }
            for region in ("en", "jp")
        },
        "supplemental_restrictions": [
            {
                "region": item.region,
                "card_id": item.card_id,
                "face_ids": list(item.face_ids),
                "reason": item.reason,
                "issue_id": item.issue_id,
                "announcement_available": item.announcement_available,
            }
            for item in restrictions
        ],
        "pending_face_regions": sum(
            len(array(face["wording"])) for face in projection.tables["face"]
        ),
        "excluded_printings": [
            {
                "region": data.region,
                "card_no": data.card_no,
                "reasons": list(item.reasons),
            }
            for item in publication.projections
            if isinstance(
                data := publication.snapshot.records[item.record_key].data, PrintingData
            )
            and item.disposition != "included"
        ],
        "product_diagnostics": list(official.diagnostics),
        "source_coverage": dict.fromkeys(
            ("qa", "errata", "cr", "restriction", "images"), "unknown"
        ),
        "incomplete_formal_gates": [
            "errata parsing and per-card corrected-text verification (#47)",
            "complete source coverage",
            "formal release gates (#34)",
            "image publication (#35)",
            "Web handoff and acceptance",
        ],
    }
    if bundle_dir is not None:

        def populate(target: Database) -> InputRecord:
            parent_record = populate_text_preview(
                target,
                catalog,
                texts,
                authored_revision=inputs.revision,
                build=context,
                vocabulary=vocabulary,
                published=(),
                languages=derived.catalog.languages,
                stores=stores,
                official=official,
            )
            adoption_record = populate_adoptions(
                target, adoptions, build=context, stores=stores
            )
            extras_record = populate_card_extras(target, extras, build=context)
            return input_record(
                context,
                (*parent_record.uses, *extras_record.uses, *adoption_record.uses),
            )

        publish_bundle(
            schema, bundle_dir, context, expected, populate, report, stores=stores
        )
    return Built(projection, ownership, content, report)
