"""Compose the pinned offline JP build without opening crawler settings or a manifest."""

from dataclasses import dataclass
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Pydantic resolves path fields at runtime
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_minimum
from sve_carddb.build_inputs import BuildContext, Revision, input_record, uses_sorted
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.image_assets import (
    plan_jp_images,
    populate_jp_assets,
    reference_uses,
    verify_asset_sources,
)
from sve_carddb.image_crop_report import crop_report
from sve_carddb.image_crops import load_image_crops
from sve_carddb.image_variants import DEFAULT_RECIPE
from sve_carddb.products import (
    FrozenProducts,
    Language,
    load_product_identities,
    load_products,
    plan_official_products,
)
from sve_carddb.products.models import Date  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves constrained fields at runtime
from sve_carddb.registry.preview import FrozenJP, plan_preview
from sve_carddb.registry.records import Hash, Instant, PrintingData, RecordData, Text
from sve_carddb.snapshot.export import Batch, Ownership
from sve_carddb.snapshot.preview import require_unknown_coverage
from sve_carddb.snapshot.project import Decisions, Projection, Settings, project
from sve_carddb.snapshot.project.source import Source
from sve_carddb.snapshot.values import array, digest, string
from sve_carddb.source_corrections import FrozenImages
from sve_carddb.text_observations import (
    FrozenTexts,
    RegionalTexts,
    Vocabulary,
    plan_text_observations,
    populate_text_preview,
    text_configuration,
)
from sve_carddb.text_observations.wording import printing_observed_texts, wording_views

if TYPE_CHECKING:
    from sve_carddb.image_assets import ImageBuild
    from sve_carddb.image_crops import ImageCrops
    from sve_carddb.registry.preview import PreviewPlan


class Inputs(RecordData):
    repo: Path
    archive: Path
    store_id: Text
    card_batch: Hash
    image_batch: Hash
    revision: Revision
    parser_version: Text
    vocabulary: Path
    languages: tuple[Language, ...]
    as_of: Date
    data_version: Text
    published_at: Instant
    feedback_url: Text
    grammar_version: Text
    normalizer_version: Text

    def batch(self) -> Batch:
        """Region scope is fixed by the preview mode, never inferred from data."""
        return Batch(self.data_version, self.published_at, ("jp",))


@dataclass(frozen=True)
class Built:
    projection: Projection
    ownership: Ownership
    input_content: bytes
    report: dict[str, JsonValue]
    confirmed_images: frozenset[str] = frozenset()


def publication_printings(plan: PreviewPlan) -> frozenset[str]:
    """Obtain only adopted printings from the publication plan's authoritative closure."""
    return frozenset(
        record.data.id
        for record in plan.included("printing")
        if isinstance(record.data, PrintingData) and record.data.region == "jp"
    )


def exclusions(plan: PreviewPlan) -> list[JsonValue]:
    """Keep outside-scope regions and wording diagnostics out of genuine exclusions."""
    result: list[JsonValue] = []
    for item in plan.projections:
        data = plan.snapshot.records[item.record_key].data
        if (
            isinstance(data, PrintingData)
            and data.region == "jp"
            and item.disposition != "included"
        ):
            result.append({"card_no": data.card_no, "reasons": list(item.reasons)})
    return result


def _image_crops(inputs: Inputs, images: ImageBuild | None) -> ImageCrops | None:
    if images is not None:
        crops = load_image_crops(
            inputs.repo / "authored", authored_revision=inputs.revision
        )
        if any(
            (item.source.archive.store_id, item.source.archive.batch_id)
            != (inputs.store_id, inputs.image_batch)
            for item in images.images
        ):
            raise ValueError("Preview images differ from the pinned image batch")
        verify_asset_sources(images, {inputs.store_id: inputs.archive}, crops=crops)
        return crops
    return None


def build(  # ruff: ignore[too-many-locals] -- one offline transaction binds the independently verified source plans
    inputs: Inputs, *, images: ImageBuild | None = None, image_root: Path | None = None
) -> Built:
    """Use archived identity/text/image evidence and repo authored, entirely read-only."""
    identity = plan_preview(
        inputs.repo / "authored",
        FrozenJP(
            inputs.archive,
            inputs.store_id,
            inputs.card_batch,
            parser_version=inputs.parser_version,
        ),
        regions=("jp",),
    )
    plan = plan_text_observations(
        identity,
        RegionalTexts(
            {
                "jp": FrozenTexts(
                    inputs.archive,
                    inputs.store_id,
                    inputs.card_batch,
                    region="jp",
                    parser_version=inputs.parser_version,
                )
            }
        ),
        images=FrozenImages(inputs.archive, inputs.store_id, inputs.image_batch),
    )
    publication = plan.publication_identity()
    catalog = load_products(inputs.repo / "authored", registry=identity.snapshot)
    stores = {inputs.store_id: inputs.archive}
    if (images is None) != (image_root is None):
        raise ValueError("Image build and asset root must be provided together")
    crops = _image_crops(inputs, images)
    identities = load_product_identities(
        inputs.repo / "authored",
        authored_revision=inputs.revision,
        catalog=catalog,
        stores=stores,
    )
    official = plan_official_products(
        identities,
        FrozenProducts(
            inputs.archive, inputs.store_id, inputs.card_batch, region="jp"
        ).pages(),
        identity,
    )
    vocabulary = Vocabulary.model_validate_json(inputs.vocabulary.read_bytes())
    vocabulary.verify()
    dependencies = {
        path.relative_to(inputs.repo).as_posix(): path.read_bytes()
        for path in (inputs.repo / "carddb/src/sve_carddb").rglob("*.py")
        if path.name != "_version.py"
    } | identities.dependencies()
    dependencies["carddb/uv.lock"] = (inputs.repo / "carddb/uv.lock").read_bytes()
    configuration = text_configuration(plan, vocabulary, ()) | {
        "product_identity": identities.configuration(),
        "preview_recipe": inputs.model_dump(
            mode="json", exclude={"repo", "archive", "vocabulary"}
        ),
        "selected_regions": ["jp"],
        "published_history": "explicit-empty-no-releases",
    }
    if crops is not None:
        dependencies.update(crops.dependencies())
        configuration["image_crop_overrides"] = crops.configuration()
    if images is not None:
        configuration["image_recipe"] = DEFAULT_RECIPE.version
    context = BuildContext.from_inputs(inputs.revision, dependencies, configuration)
    if crops is not None:
        crops.verify_context(context)
    # Adopted JP art can use the shared art DDL; DDL capability is not region coverage.
    with create_database(compile_minimum(include_en=True)) as db:
        with db.transaction():
            record = populate_text_preview(
                db,
                catalog,
                plan,
                authored_revision=inputs.revision,
                build=context,
                vocabulary=vocabulary,
                published=(),
                languages=inputs.languages,
                stores=stores,
                official=official,
            )
            image_report: dict[str, JsonValue] | None = None
            if images is not None and image_root is not None:
                references = plan_jp_images(
                    db,
                    identity,
                    FrozenSources(inputs.archive, inputs.store_id, inputs.card_batch),
                )
                expected = uses_sorted(
                    (*record.uses, *images.source_uses(), *reference_uses(references))
                )
                added = populate_jp_assets(db, images, references, image_root)
                record = input_record(context, (*record.uses, *added))
                record.verify(db, context, expected)
                image_report = {
                    key: value
                    for key, value in images.report(references).items()
                    if key not in {"elapsed_milliseconds", "cache_hits"}
                }
                if crops is not None:
                    image_report["crop_overrides"] = crop_report(
                        crops, images, references, db
                    )
        decisions = Decisions().with_text_views(
            wording_views(db, plan), printing_observed_texts(db, plan)
        )
        projection = project(
            db,
            regions=("jp",),
            as_of=inputs.as_of,
            settings=Settings(
                inputs.feedback_url, inputs.grammar_version, inputs.normalizer_version
            ),
            decisions=decisions,
            publication_printings=publication_printings(publication),
        )
        require_unknown_coverage(projection)
        ownership = Ownership.from_database(db, projection)
        public_images = {row["id"] for row in projection.tables["image_asset"]}
        # project() runs db.verify(): third-party approval requires confirmed review and matching source evidence.
        # This set carries that DB guarantee; the writer does not audit private review decisions.
        confirmed_images = frozenset(
            string(row["id"])
            for row in Source(db).rows("image_asset", "id,origin,publication_state")
            if row["id"] in public_images
            and row["origin"] == "third_party"
            and row["publication_state"] == "approved"
        )
    content = record.content()
    report: dict[str, JsonValue] = {
        "input_sha256": digest(content),
        "card_batch": inputs.card_batch,
        "image_batch": inputs.image_batch,
        "regions": ["jp"],
        "counts": {table: len(rows) for table, rows in projection.tables.items()},
        "excluded_printings": exclusions(publication),
        "pending_face_regions": sum(
            len(array(face["wording"])) for face in projection.tables["face"]
        ),
        "errata_link_printings": len(
            {
                observation.printing_id
                for observation in plan.observations
                if observation.card.has_errata_link
                and observation.printing_id in publication_printings(publication)
            }
        ),
        "source_coverage": dict.fromkeys(
            ("qa", "errata", "cr", "restriction"), "unknown"
        ),
        "incomplete_formal_gates": [
            "QA/errata/CR/restriction source coverage",
            "curated vocabulary and translation review",
            "image manifest integration (#35)",
            "formal release gates and append-only index (#34)",
            "M3 browser acceptance",
        ],
    }
    if image_report is not None:
        report["image_assets"] = image_report
        report["incomplete_formal_gates"] = [
            item
            for item in array(report["incomplete_formal_gates"])
            if item != "image manifest integration (#35)"
        ]
    return Built(projection, ownership, content, report, confirmed_images)
