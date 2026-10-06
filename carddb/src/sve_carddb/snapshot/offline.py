"""Compose regional card-page supplements without granting formal release coverage."""

from dataclasses import dataclass, replace
from pathlib import Path
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, model_validator

from sve_carddb.build_bundle import publish_bundle
from sve_carddb.build_db import create_database
from sve_carddb.build_db.current import compile_current_build
from sve_carddb.build_db.t1 import MINIMUM_CAPABILITIES
from sve_carddb.build_inputs import (
    BuildContext,
    Revision,
    input_record,
    insert_raw_sources,
    uses_sorted,
)
from sve_carddb.card_extras import (
    FrozenCardExtras,
    applicable_reskin_regions,
    plan_card_extras,
    populate_card_extras,
    require_card_extras_ready,
)
from sve_carddb.catalog.adoption_models import Batch as SourceBatch
from sve_carddb.catalog.adoption_sources import SOURCE_RECIPE_PATHS, PinnedRepository
from sve_carddb.image_variants import DEFAULT_RECIPE
from sve_carddb.products import (
    FrozenProducts,
    load_product_identities,
    load_products,
    plan_official_products,
)
from sve_carddb.products.models import Date
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
from sve_carddb.snapshot.offline_images import prepare_images
from sve_carddb.snapshot.offline_names import composer
from sve_carddb.snapshot.project import Decisions, Projection, Settings, project
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.source_corrections import FrozenImages
from sve_carddb.template_parameter_rules.current import load as load_rules
from sve_carddb.template_parameters.current_references import adopted
from sve_carddb.template_translations.current import read_templates, validate_templates
from sve_carddb.template_translations.current_build import apply as apply_templates
from sve_carddb.template_translations.current_references import (
    References as TemplateReferences,
)
from sve_carddb.template_translations.current_sources import Sources as TemplateSources
from sve_carddb.text_observations import (
    FrozenTexts,
    RegionalTexts,
    plan_text_observations,
    populate_text_preview,
    text_configuration,
)
from sve_carddb.text_observations.composition import text_preview_uses
from sve_carddb.text_observations.wording import printing_observed_texts, wording_views
from sve_carddb.translations.current_models import (
    ChoiceRecord,
    ConceptRecord,
    TermRecord,
)
from sve_carddb.translations.current_names import prepare as prepare_names
from sve_carddb.translations.flavor import apply as apply_flavor
from sve_carddb.translations.flavor import load as load_flavor
from sve_carddb.translations.models import EffectTerm, SourceValue
from sve_carddb.translations.sources import CODE_PATH as TRANSLATION_CODE
from sve_carddb.translations.sources import Sources as TranslationSources

if TYPE_CHECKING:
    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import InputRecord, Source, SourceUse
    from sve_carddb.card_extras import ErrataPage, ExtrasPlan
    from sve_carddb.catalog.adoption_importer import AdoptionInputs
    from sve_carddb.catalog.current import Prepared
    from sve_carddb.catalog.projection import CatalogProjection
    from sve_carddb.image_assets import ImageBuild
    from sve_carddb.products import ProductIdentities
    from sve_carddb.registry.records import CorrectionEvidence
    from sve_carddb.template_translations.current import Validated
    from sve_carddb.text_observations.vocabulary import Vocabulary
    from sve_carddb.translations.current_names import Names


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
    for name in ("carddb/uv.lock", "carddb/pyproject.toml"):
        dependencies[name] = (repo / name).read_bytes()
    return dependencies


def _catalog_source_recipes(
    revision: str, dependencies: dict[str, bytes]
) -> dict[str, JsonValue]:
    """Pin supported current implementations independently of historical receipts."""
    recipes: dict[str, JsonValue] = {}
    for parser, path in SOURCE_RECIPE_PATHS.items():
        content = dependencies.get(path)
        if content is None:
            raise ValueError("Offline catalog parser dependency is absent")
        recipes[parser] = {
            "version": parser,
            "program_revision": revision,
            "code_path": path,
            "code_hash": digest(content),
            "config": {},
            "config_hash": digest(canonical({})),
        }
    return recipes


def _translation_source_recipes(
    revision: str, dependencies: dict[str, bytes]
) -> dict[str, JsonValue]:
    """Bind exact glossary projections to the current implementation and provider."""
    code = dependencies.get(TRANSLATION_CODE)
    if code is None:
        raise ValueError("Offline translation parser dependency is absent")
    return {
        "translation-" + provider + "-v1": {
            "version": "translation-" + provider + "-v1",
            "program_revision": revision,
            "code_path": TRANSLATION_CODE,
            "code_hash": digest(code),
            "config": {"provider": provider},
            "config_hash": digest(canonical({"provider": provider})),
        }
        for provider in ("en", "jp", "sv1", "svwb")
    }


def _translation_sources(
    inputs: AdoptionInputs, build: BuildContext, stores: dict[str, Path]
) -> tuple[dict[str, str], TranslationSources]:
    """Collect exactly the current frozen terms and values, without historical decisions."""
    translation = inputs.translation_inputs()
    sources = TranslationSources(stores, inputs.repository, build)
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
    inputs: AdoptionInputs, build: BuildContext, stores: dict[str, Path], db: Database
) -> Names | None:
    translation = inputs.translation_inputs()
    if translation is None:
        return None
    originals, sources = _translation_sources(inputs, build, stores)
    return prepare_names(translation.load(), originals, translation, sources, db)


def _translation_uses(
    inputs: AdoptionInputs, build: BuildContext, stores: dict[str, Path]
) -> tuple[SourceUse, ...]:
    """Check the current source closure independently of database insertion."""
    if inputs.translation_inputs() is None:
        return ()
    return uses_sorted(_translation_sources(inputs, build, stores)[1].uses)


def _templates(
    inputs: AdoptionInputs,
    build: BuildContext,
    stores: dict[str, Path],
    vocabulary: Vocabulary,
    batch: SourceBatch,
) -> Validated | None:
    """Template source positions come from this build's sealed JP batch, not from Git."""
    if inputs.translation_inputs() is None:
        return None
    repository = PinnedRepository(inputs.repository)
    templates = read_templates(repository, inputs.authored_revision)
    if not templates.records:
        return None
    found = adopted(
        templates.glossary, TranslationSources(stores, inputs.repository, build)
    )
    references = TemplateReferences(
        card_names=found.card_names,
        terms=found.terms,
        vocabulary=vocabulary,
        pins=found.pins,
    )
    rules = load_rules(repository, inputs.authored_revision)
    return validate_templates(
        templates, TemplateSources(stores, references, rules), (batch,)
    )


def _prepare_catalog(
    inputs: AdoptionInputs, build: BuildContext, stores: dict[str, Path]
) -> Prepared:
    """Native offline builds consume current values, never receipt envelopes."""
    from sve_carddb.catalog.current import CURRENT_FORMAT, prepare  # ruff: ignore[import-outside-top-level] -- initialize the shared text interner before catalog modules

    snapshots = inputs.load()
    if any(
        snapshot.entry != "catalog-adoptions"
        or object_value(parse(snapshot.index_content)).get("catalog_adoption_format")
        != CURRENT_FORMAT
        or any(
            object_value(parse(shard.content)).get("catalog_adoption_format")
            != CURRENT_FORMAT
            for shard in snapshot.shards
        )
        for snapshot in snapshots
    ):
        raise ValueError("Offline catalog requires current format 2 inputs")
    configuration = object_value(parse(build.configuration.encode()))
    if any(
        configuration.get(key) != value for key, value in inputs.configuration().items()
    ):
        raise ValueError("Build configuration does not pin adoption inputs")
    return prepare(snapshots, inputs.repository, build, stores)


def _derive_adoptions(
    inputs: AdoptionInputs, context: BuildContext, stores: dict[str, Path]
) -> CatalogProjection:
    """Derive current values before any candidate text is written."""
    derived = _prepare_catalog(inputs, context, stores).projection
    if not {"en", "ja"} <= {language.code for language in derived.catalog.languages}:
        raise ValueError("Offline launch requires adopted EN and JA languages")
    return derived


def _adoption_uses(
    inputs: AdoptionInputs, build: BuildContext, stores: dict[str, Path]
) -> tuple[SourceUse, ...]:
    """Replay current frozen evidence independently of database insertion."""
    return uses_sorted(_prepare_catalog(inputs, build, stores).sources.uses)


def _populate_adoptions(
    db: Database,
    inputs: AdoptionInputs,
    *,
    build: BuildContext,
    stores: dict[str, Path],
) -> InputRecord:
    """Write the current catalog and glossary under the current schema only."""
    from sve_carddb.catalog.current import populate  # ruff: ignore[import-outside-top-level] -- initialize the shared text interner before catalog modules
    from sve_carddb.translations.importer import populate_glossary  # ruff: ignore[import-outside-top-level] -- glossary import shares the catalog/text boundary

    prepared = _prepare_catalog(inputs, build, stores)
    insert_raw_sources(db, (use.source for use in prepared.sources.uses))
    populate(db, prepared.snapshots, prepared, inputs.authored_revision)
    translation = inputs.translation_inputs()
    translation_uses = (
        ()
        if translation is None
        else populate_glossary(db, translation, build=build, stores=stores).uses
    )
    uses = (*prepared.sources.uses, *translation_uses)
    result = input_record(build, uses)
    result.verify(db, build, uses, complete=False)
    return result


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


def build(  # ruff: ignore[too-many-locals, complex-structure, too-many-statements] -- one transaction and its sealed replay bind the independently verified domain plans
    inputs: Inputs,
    *,
    errata: tuple[ErrataPage, ...] = (),
    bundle_dir: Path | None = None,
    images: ImageBuild | None = None,
    image_root: Path | None = None,
) -> Built:
    """Build both regions from sealed sources, retaining every diagnostic source use."""
    from sve_carddb.catalog.adoption_importer import AdoptionInputs  # ruff: ignore[import-outside-top-level] -- load after text modules initialize the shared interner

    if bundle_dir is not None:
        for protected in (inputs.repo, inputs.archive):
            output, source = bundle_dir.resolve(), protected.resolve()
            if output.is_relative_to(source) or source.is_relative_to(output):
                raise ValueError(
                    "Offline bundle must be disjoint from immutable inputs"
                )
    names = composer(inputs)
    mounted = prepare_images(inputs, images, image_root)
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
        inputs.repo / "authored",
        inputs.repo,
        inputs.revision,
        ("catalog-adoptions",),
        include_translations=True,
    )
    configuration = adoptions.configuration() | {
        "catalog_source_recipes": _catalog_source_recipes(
            inputs.revision, dependencies
        ),
        "translation_recipes": _translation_source_recipes(
            inputs.revision, dependencies
        ),
        "product_identity": identities.configuration(),
        "offline_recipe": inputs.model_dump(mode="json", exclude={"repo", "archive"}),
        "selected_regions": ["en", "jp"],
        "published_history": "explicit-empty-no-releases",
    }
    if names is not None:
        dependencies.update(names.dependencies())
        configuration |= names.configuration(inputs)
    if mounted is not None:
        configuration["image_recipe"] = DEFAULT_RECIPE.version
    context = BuildContext.from_inputs(inputs.revision, dependencies, configuration)
    schema = compile_current_build(
        (
            *MINIMUM_CAPABILITIES,
            "en",
            "translation_evidence",
            "translation_names",
            "translation_templates",
        )
    )
    derived = _derive_adoptions(adoptions, context, stores)
    vocabulary = derived.vocabulary
    configuration |= text_configuration(texts, vocabulary, ())
    context = BuildContext.from_inputs(inputs.revision, dependencies, configuration)
    flavor = load_flavor(inputs.repo / "authored")
    templates = _templates(
        adoptions, context, stores, vocabulary, SourceBatch(batch_id=jp.card_batch)
    )
    adoption_uses = (
        *_adoption_uses(adoptions, context, stores),
        *_translation_uses(adoptions, context, stores),
    )
    image_report: dict[str, JsonValue] | None = None
    name_result = None
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
            link_result = (
                None
                if names is None
                else names.populate_links(db, context=context, stores=stores)
            )
            adopted = _populate_adoptions(db, adoptions, build=context, stores=stores)
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
            name_expected: tuple[SourceUse, ...] = ()
            if names is not None:
                replay = _current_names(adoptions, context, stores, db)
                assert replay is not None
                if link_result is not None:
                    link_result = replace(
                        link_result,
                        record=input_record(context, link_result.record.uses),
                    )
                name_expected = names.expected(
                    db,
                    texts,
                    context=context,
                    stores=stores,
                    replay=replay,
                    links=link_result,
                )
                name_result = names.populate(
                    db,
                    texts,
                    context=context,
                    stores=stores,
                    replay=replay,
                    links=link_result,
                )
            flavor_report = apply_flavor(db, flavor)
            template_report = (
                None if templates is None else apply_templates(db, templates, LANG)
            )
            link_uses = () if link_result is None else link_result.record.uses
            name_uses = () if name_result is None else name_result.record.uses
            expected = uses_sorted(
                (
                    *text_preview_uses(catalog, texts, stores, official=official),
                    *extras.source_uses(),
                    *adoption_uses,
                    *link_uses,
                    *name_expected,
                )
            )
            record = input_record(
                context,
                (*parents.uses, *added.uses, *adopted.uses, *link_uses, *name_uses),
            )
            record.verify(db, context, expected)
            if mounted is not None:
                record, image_report = mounted.populate(
                    db, inputs, identity, context, record
                )
                expected = record.uses
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
            display_bindings=() if name_result is None else name_result.bindings,
            private_digital=names is not None,
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
    report["flavor_translations"] = dict[str, JsonValue](flavor_report.payload())
    report["effect_translations"] = (
        None if template_report is None else template_report.payload()
    )
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
            replay_links = (
                None
                if names is None
                else names.populate_links(target, context=context, stores=stores)
            )
            adoption_record = _populate_adoptions(
                target, adoptions, build=context, stores=stores
            )
            extras_record = populate_card_extras(target, extras, build=context)
            replay_names_result = None
            if names is not None:
                replay = _current_names(adoptions, context, stores, target)
                assert replay is not None
                replay_names_result = names.populate(
                    target,
                    texts,
                    context=context,
                    stores=stores,
                    replay=replay,
                    links=replay_links,
                )
            apply_flavor(target, flavor)
            if templates is not None:
                apply_templates(target, templates, LANG)
            complete = input_record(
                context,
                (
                    *parent_record.uses,
                    *extras_record.uses,
                    *adoption_record.uses,
                    *(() if replay_links is None else replay_links.record.uses),
                    *(
                        ()
                        if replay_names_result is None
                        else replay_names_result.record.uses
                    ),
                ),
            )
            if mounted is not None:
                complete, _ = mounted.populate(
                    target, inputs, identity, context, complete
                )
            return complete

        publish_bundle(
            schema, bundle_dir, context, expected, populate, report, stores=stores
        )
    return Built(projection, ownership, content, report)
