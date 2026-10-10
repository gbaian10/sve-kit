"""Compose regional card-page supplements without granting formal release coverage."""

from dataclasses import dataclass, replace
from pathlib import Path
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, model_validator

from sve_carddb.build import create_database
from sve_carddb.build.output import save as save_build
from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.build.t1 import MINIMUM_CAPABILITIES, compile_build
from sve_carddb.contracts.snapshot import validate
from sve_carddb.core.authored import authored_root
from sve_carddb.core.json import array, digest, object_value, parse
from sve_carddb.core.models import Hash, Instant, RecordData, Text
from sve_carddb.core.provenance import BuildContext, Revision, input_record
from sve_carddb.core.regions import Region
from sve_carddb.domains.card_extras import (
    FrozenCardExtras,
    applicable_reskin_regions,
    plan_card_extras,
    populate_card_extras,
    require_card_extras_ready,
)
from sve_carddb.domains.products import (
    FrozenProducts,
    load_product_identities,
    load_products,
    plan_official_products,
)
from sve_carddb.domains.products.models import Date
from sve_carddb.domains.registry.preview import (
    FrozenEN,
    FrozenJP,
    FrozenRegions,
    plan_preview,
)
from sve_carddb.domains.registry.records import PrintingData
from sve_carddb.domains.rulings.reader import load as load_rulings
from sve_carddb.domains.rulings.storage import PROFILE as RULING_PROFILE
from sve_carddb.domains.rulings.storage import write as write_rulings
from sve_carddb.domains.source_corrections import FrozenImages
from sve_carddb.domains.text_observations import (
    FrozenTexts,
    RegionalTexts,
    TextObservations,
)
from sve_carddb.domains.text_observations.composition import populate_text_preview
from sve_carddb.domains.translations.corrected_sources import Corrections
from sve_carddb.domains.translations.flavor import apply as apply_flavor
from sve_carddb.domains.translations.flavor import load as load_flavor
from sve_carddb.domains.translations.four_layer_build import Settings as FrameSettings
from sve_carddb.domains.translations.four_layer_build import apply as apply_templates
from sve_carddb.domains.translations.glossary.records import (
    ChoiceRecord,
    ConceptRecord,
    TermRecord,
)
from sve_carddb.domains.translations.jp_sources import effect_bindings
from sve_carddb.domains.translations.models import EffectTerm, SourceValue
from sve_carddb.domains.translations.names.resolve import prepare as prepare_names
from sve_carddb.domains.translations.recognition.rules import load as load_rules
from sve_carddb.domains.translations.sources import Sources as TranslationSources
from sve_carddb.export.project import Decisions, Projection, Settings, project
from sve_carddb.export.transport import Batch, Ownership
from sve_carddb.images.checks import ImageChecks
from sve_carddb.images.variants import DEFAULT_RECIPE
from sve_carddb.workflows.offline_images import prepare_images
from sve_carddb.workflows.offline_names import composer

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.core.provenance import InputRecord, Source
    from sve_carddb.domains.card_extras import ErrataPage, ExtrasPlan
    from sve_carddb.domains.catalog.adoption_importer import AdoptionInputs
    from sve_carddb.domains.catalog.loader import Prepared
    from sve_carddb.domains.registry.records import CorrectionEvidence
    from sve_carddb.domains.registry.snapshot import RegistrySnapshot
    from sve_carddb.domains.translations.four_layer_authored import (
        Inputs as FrameInputs,
    )
    from sve_carddb.domains.translations.names.resolve import Names
    from sve_carddb.images.assets import ImageBuild
    from sve_carddb.ingest.archive.frozen_sources import FrozenBatches


# Templates and flavor translate Japanese source text into this display language.
LANG = "zh-Hant"


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
    name_policy: Literal["approved-frozen-v1"] | None = None

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


@dataclass(frozen=True)
class Built:
    projection: Projection
    ownership: Ownership
    input_content: bytes
    report: dict[str, JsonValue]


class RegionalImages:
    def __init__(self, inputs: Inputs, checks: ImageChecks) -> None:
        self.providers = {
            pin.region: FrozenImages(
                inputs.archive,
                inputs.store_id,
                pin.image_batch,
                sources=checks.batch(inputs.archive, inputs.store_id, pin.image_batch),
            )
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


def _translation_sources(
    inputs: AdoptionInputs,
    build: BuildContext,
    stores: dict[str, Path],
    sources: TranslationSources | None = None,
) -> tuple[dict[str, str], TranslationSources]:
    """Collect exactly the current frozen terms and values, without historical decisions."""
    translation = inputs.translation_inputs()
    sources = sources or TranslationSources(stores, inputs.repository, build)
    originals: dict[str, str] = {}
    if translation is None:
        return originals, sources
    for record in translation.load().current_records():
        if isinstance(record, TermRecord):
            originals[record.data.id] = (
                sources.text(record.data.source_ref, record.data.source_span)[1]
                if record.data.source_ref is not None
                else str(record.data.authored_source_ja)
            )
        elif isinstance(record, ConceptRecord):
            sources.text(record.data.source_ref)
        elif isinstance(record, ChoiceRecord) and record.data.value is not None:
            if isinstance(record.data.value, SourceValue):
                sources.text(record.data.value.source_ref, record.data.value.span)
            for relation in record.data.concept_evidence:
                sources.text(
                    relation.jp_ref,
                    relation.jp_span if isinstance(relation, EffectTerm) else None,
                )
                sources.text(
                    relation.target_ref,
                    relation.target_span if isinstance(relation, EffectTerm) else None,
                )
    return originals, sources


def _current_names(
    inputs: AdoptionInputs,
    build: BuildContext,
    stores: dict[str, Path],
    db: Database,
    sources: TranslationSources,
) -> Names | None:
    translation = inputs.translation_inputs()
    if translation is None:
        return None
    originals, sources = _translation_sources(inputs, build, stores, sources)
    return prepare_names(translation.load(), originals, translation, sources, db)


def _templates(inputs: AdoptionInputs) -> FrameInputs | None:
    """The shared closure performs A; exact owner compilation performs B inside this build."""
    translation = inputs.translation_inputs()
    if translation is None:
        return None
    current = translation.load().four_layer
    return (
        current if any(r.kind == "sentence_template" for r in current.records) else None
    )


def _prepare_catalog(
    inputs: AdoptionInputs,
    build: BuildContext,
    stores: dict[str, Path],
    registry: RegistrySnapshot | None = None,
    *,
    batches: FrozenBatches | None = None,
) -> Prepared:
    """Native offline builds consume current values, never receipt envelopes."""
    from sve_carddb.domains.catalog.loader import prepare  # ruff: ignore[import-outside-top-level] -- initialize the shared text interner before catalog modules

    snapshots = inputs.load()
    if any(snapshot.entry != "catalog/adoptions" for snapshot in snapshots):
        raise ValueError("Offline catalog requires catalog/adoptions inputs")
    configuration = object_value(parse(build.configuration.encode()))
    if any(
        configuration.get(key) != value for key, value in inputs.configuration().items()
    ):
        raise ValueError("Build configuration does not pin adoption inputs")
    return prepare(
        snapshots, inputs.repository, build, stores, registry, batches=batches
    )


def _populate_adoptions(
    db: Database,
    inputs: AdoptionInputs,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    prepared: Prepared,
    sources: TranslationSources,
) -> InputRecord:
    """Write the current catalog and glossary under the current schema only."""
    from sve_carddb.domains.catalog.loader import populate  # ruff: ignore[import-outside-top-level] -- initialize the shared text interner before catalog modules
    from sve_carddb.domains.translations.glossary.importer import populate_glossary  # ruff: ignore[import-outside-top-level] -- glossary import shares the catalog/text boundary

    insert_raw_sources(db, (use.source for use in prepared.sources.uses))
    populate(db, prepared.snapshots, prepared, inputs.authored_revision)
    translation = inputs.translation_inputs()
    translation_uses = (
        ()
        if translation is None
        else populate_glossary(
            db, translation, build=build, stores=stores, sources=sources
        ).uses
    )
    uses = (*prepared.sources.uses, *translation_uses)
    return input_record(build, uses)


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


def build(  # ruff: ignore[too-many-locals, complex-structure, too-many-statements, too-many-branches] -- one transaction combines the checked domain plans and saves its completed database
    inputs: Inputs,
    *,
    errata: tuple[ErrataPage, ...] = (),
    bundle_dir: Path | None = None,
    images: ImageBuild | None = None,
    image_root: Path | None = None,
    image_checks: ImageChecks | None = None,
) -> Built:
    """Build both regions from sealed sources, retaining every diagnostic source use."""
    from sve_carddb.domains.catalog.adoption_importer import AdoptionInputs  # ruff: ignore[import-outside-top-level] -- load after text modules initialize the shared interner

    if bundle_dir is not None:
        for protected in (inputs.repo, inputs.archive):
            output, source = bundle_dir.resolve(), protected.resolve()
            if output.is_relative_to(source) or source.is_relative_to(output):
                raise ValueError(
                    "Offline bundle must be disjoint from immutable inputs"
                )
    names = composer(inputs)
    image_checks = image_checks or ImageChecks()
    batches = image_checks.batches
    mounted = prepare_images(inputs, images, image_root, image_checks)
    if (
        mounted is not None
        and bundle_dir is not None
        and (
            bundle_dir.resolve().is_relative_to(mounted.root.resolve())
            or mounted.root.resolve().is_relative_to(bundle_dir.resolve())
        )
    ):
        raise ValueError("Offline bundle and image assets must be disjoint")
    en, jp = inputs.sources
    identity = plan_preview(
        authored_root(inputs.repo),
        FrozenRegions(
            en=FrozenEN(
                inputs.archive,
                inputs.store_id,
                en.card_batch,
                parser_version=en.parser_version,
                sources=batches.batch(inputs.archive, inputs.store_id, en.card_batch),
            ),
            jp=FrozenJP(
                inputs.archive,
                inputs.store_id,
                jp.card_batch,
                parser_version=jp.parser_version,
                sources=batches.batch(inputs.archive, inputs.store_id, jp.card_batch),
            ),
        ),
        regions=("en", "jp"),
    )
    observations = TextObservations(
        identity,
        RegionalTexts(
            {
                pin.region: FrozenTexts(
                    inputs.archive,
                    inputs.store_id,
                    pin.card_batch,
                    region=pin.region,
                    parser_version=pin.parser_version,
                    sources=batches.batch(
                        inputs.archive, inputs.store_id, pin.card_batch
                    ),
                )
                for pin in inputs.sources
            }
        ),
        images=RegionalImages(inputs, image_checks),
    )
    texts = observations.plan
    publication = texts.publication_identity()
    printings = frozenset(
        record.data.id
        for record in publication.included("printing")
        if isinstance(record.data, PrintingData)
    )
    catalog = load_products(authored_root(inputs.repo), registry=identity.snapshot)
    stores = {inputs.store_id: inputs.archive}
    identities = load_product_identities(
        authored_root(inputs.repo),
        authored_revision=inputs.revision,
        catalog=catalog,
        stores=stores,
        batches=batches,
    )
    official = plan_official_products(
        identities,
        tuple(
            page
            for pin in inputs.sources
            for page in FrozenProducts(
                inputs.archive,
                inputs.store_id,
                pin.card_batch,
                region=pin.region,
                sources=batches.batch(inputs.archive, inputs.store_id, pin.card_batch),
            ).pages()
        ),
        identity,
    )
    pages = tuple(
        page
        for pin in inputs.sources
        for page in FrozenCardExtras(
            inputs.archive,
            inputs.store_id,
            pin.card_batch,
            region=pin.region,
            sources=batches.batch(inputs.archive, inputs.store_id, pin.card_batch),
        ).pages()
    )
    adoptions = AdoptionInputs(
        authored_root(inputs.repo),
        inputs.repo,
        inputs.revision,
        ("catalog/adoptions",),
        include_translations=True,
    )
    configuration = adoptions.configuration() | {
        "product_identity": identities.configuration(),
        "offline_recipe": inputs.model_dump(
            mode="json",
            exclude={"repo", "archive"},
        ),
        "selected_regions": ["en", "jp"],
        "published_history": "explicit-empty-no-releases",
    }
    rulings = load_rulings(inputs.repo)
    configuration["ruling_references"] = {
        "profile": RULING_PROFILE,
        "documents": [item.source.model_dump(mode="json") for item in rulings],
    }
    if names is not None:
        configuration |= names.configuration(inputs)
    if mounted is not None:
        configuration["image_recipe"] = DEFAULT_RECIPE.version
    context = BuildContext.from_inputs(inputs.revision, configuration)
    schema = compile_build(
        (
            *MINIMUM_CAPABILITIES,
            "en",
            "translation_evidence",
            "translation_names",
            "translation_templates",
            "rulings",
        )
    )
    prepared = _prepare_catalog(
        adoptions, context, stores, identity.snapshot, batches=batches
    )
    derived = prepared.projection
    if not {"en", "ja"} <= {language.code for language in derived.catalog.languages}:
        raise ValueError("Offline launch requires adopted EN and JA languages")
    vocabulary = derived.vocabulary
    configuration |= observations.configuration(vocabulary, ())
    context = BuildContext.from_inputs(inputs.revision, configuration)
    flavor = load_flavor(authored_root(inputs.repo))
    templates = _templates(adoptions)
    translation_sources = TranslationSources(
        stores,
        inputs.repo,
        context,
        identity.snapshot,
        batches=batches,
        corrections=None if texts.corrections is None else Corrections(texts),
    )
    image_report: dict[str, JsonValue] | None = None
    name_result = None
    with create_database(schema) as db:
        with db.transaction():
            parents = populate_text_preview(
                db,
                catalog,
                observations,
                authored_revision=inputs.revision,
                build=context,
                vocabulary=vocabulary,
                published=(),
                languages=derived.catalog.languages,
                stores=stores,
                official=official,
                batches=batches,
            )
            link_result = (
                None
                if names is None
                else names.populate_links(
                    db, context=context, stores=stores, sources=translation_sources
                )
            )
            adopted = _populate_adoptions(
                db,
                adoptions,
                build=context,
                stores=stores,
                prepared=prepared,
                sources=translation_sources,
            )
            extras = plan_card_extras(db, pages, errata=errata)
            # The parent record is private staging; only the complete context is emitted.
            context = BuildContext.from_inputs(
                inputs.revision,
                configuration
                | {
                    "card_extras": extras.configuration(),
                },
            )
            name_sources = translation_sources.stage(context)
            added = populate_card_extras(db, extras, build=context)
            if names is not None:
                replay = _current_names(adoptions, context, stores, db, name_sources)
                assert replay is not None
                if link_result is not None:
                    link_result = replace(
                        link_result,
                        record=input_record(context, link_result.record.uses),
                    )
                name_result = names.populate(
                    db,
                    texts,
                    replay=replay,
                    links=link_result,
                    sources=name_sources,
                )
            flavor_report = apply_flavor(db, flavor)
            translation = adoptions.translation_inputs()
            template_report = (
                None
                if templates is None or translation is None
                else apply_templates(
                    db,
                    templates,
                    translation.load(),
                    FrameSettings(
                        name_sources,
                        vocabulary,
                        load_rules(inputs.repo),
                        jp.card_batch,
                        inputs.revision,
                        LANG,
                    ),
                )
            )
            link_uses = () if link_result is None else link_result.record.uses
            ruling_report = write_rulings(db, rulings, inputs.revision)
            name_uses = () if name_result is None else name_result.record.uses
            record = input_record(
                context,
                (
                    *parents.uses,
                    *added.uses,
                    *adopted.uses,
                    *link_uses,
                    *name_uses,
                    *name_sources.uses,
                ),
            )
            if mounted is not None:
                record, image_report = mounted.populate(
                    db, inputs, identity, context, record
                )
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
        views = observations.views(db)
        decisions = replace(
            Decisions(),
            related_regions=applicable_reskin_regions(db, texts, vocabulary=vocabulary),
            supplemental_restrictions=restrictions,
            display_bindings=(
                *(() if name_result is None else name_result.bindings),
                *effect_bindings(db),
            ),
            private_digital=names is not None,
        ).with_text_views(views.wording, views.observed)
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
                    data := publication.snapshot.records[item.record_key].data,
                    PrintingData,
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
        report["flavor_translations"] = dict[str, JsonValue](flavor_report.payload())
        report["effect_translations"] = (
            None if template_report is None else template_report.payload()
        )
        report["ruling_references"] = ruling_report.payload()
        if name_result is not None:
            report["name_application"] = name_result.report
        if image_report is not None:
            report["image_assets"] = image_report
            report["incomplete_formal_gates"] = [
                item
                for item in array(report["incomplete_formal_gates"])
                if item != "image publication (#35)"
            ]
        if bundle_dir is not None:
            save_build(db, bundle_dir, content, report)
        return Built(projection, ownership, content, report)
