"""Build independent image assets and immutable metadata with one version authority."""

from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves annotations at runtime
from typing import TYPE_CHECKING, Annotated

import typer
from pydantic import JsonValue

from sve_carddb.core.authored import authored_root
from sve_carddb.core.json import array, canonical, digest, integer, object_value, string
from sve_carddb.domains.registry.preview import (
    FrozenEN,
    FrozenJP,
    FrozenRegions,
    plan_preview,
)
from sve_carddb.domains.registry.records import AllocationData, FaceData, PrintingData
from sve_carddb.domains.source_corrections import FrozenImages
from sve_carddb.domains.text_observations import (
    FrozenTexts,
    RegionalTexts,
    TextObservations,
)
from sve_carddb.export import preview
from sve_carddb.export.image_index import (
    MEDIA_TABLES,
    ImageBinding,
    ImageEvidence,
    ImageIndex,
    ImagePin,
)
from sve_carddb.export.media import prepare_media
from sve_carddb.export.media_urls import image_path
from sve_carddb.export.preview import Roots
from sve_carddb.export.preview.media_state import STATE, commit, reserve
from sve_carddb.export.project import Projection
from sve_carddb.export.read_api import POINTER
from sve_carddb.images.assets import (
    MAX_WORKERS,
    ImageBuild,
    PreviewRoots,
    build_regional_assets,
)
from sve_carddb.images.checks import ImageChecks
from sve_carddb.images.crop_report import CropBinding, crop_report
from sve_carddb.images.crops import load_image_crops
from sve_carddb.images.variants import DEFAULT_RECIPE
from sve_carddb.ingest.archive.frozen_sources import FrozenBatches, FrozenSources
from sve_carddb.parse.pages.official_jp import image_url
from sve_carddb.workflows.offline import Inputs

if TYPE_CHECKING:
    from sve_carddb.core.provenance import Source
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.registry.records import CorrectionEvidence
    from sve_carddb.domains.text_observations.plan import TextPlan
    from sve_carddb.export.media import MediaPlan
    from sve_carddb.export.project.source import Record

app = typer.Typer(
    no_args_is_help=True, help="Build image assets and pinned image metadata."
)


class RegionalImages:
    def __init__(
        self, inputs: Inputs, batches: dict[Region, FrozenSources], *, verify_all: bool
    ) -> None:
        self.providers = {
            pin.region: FrozenImages(
                inputs.archive,
                inputs.store_id,
                pin.image_batch,
                sources=batches[pin.region],
                verify_raw=verify_all,
            )
            for pin in inputs.sources
        }

    def image(self, evidence: CorrectionEvidence) -> Source:
        """Resolve corrections only in their explicitly pinned regional image batch."""
        return self.providers[evidence.region].image(evidence)


def media_projection(  # ruff: ignore[too-many-locals] -- source bindings and public rows are built together before version allocation
    texts: TextPlan, images: ImageBuild
) -> tuple[Projection, tuple[ImageBinding, ...], tuple[CropBinding, ...]]:
    """Use the same source-bound image identity as the build database projection."""
    identity = texts.publication_identity()
    allocations = {
        item.data.printing_id: item.data.int_id
        for item in identity.included("card_int_id")
        if isinstance(item.data, AllocationData)
    }
    faces = [
        item.data
        for item in identity.included("face")
        if isinstance(item.data, FaceData)
    ]
    by_url = {(item.region, item.source.url): item for item in images.images}
    observations = {
        (item.printing_id, item.face_id): item
        for item in texts.observations
        if item.card.source.id == identity.evidence[item.region, item.card_no].source.id
    }
    assets: dict[str, Record] = {}
    variants: dict[tuple[str, str], Record] = {}
    rows: list[Record] = []
    prints: list[Record] = []
    bindings: list[ImageBinding] = []
    crop_bindings: list[CropBinding] = []
    ordinals = {face.id: face.ordinal for face in faces}
    for record in identity.included("printing"):
        data = record.data
        if not isinstance(data, PrintingData):
            continue
        prints.append(
            {
                "id": data.id,
                "int_id": allocations[data.id],
                "faces": [{"face_id": m.face_id} for m in data.source_face_map],
            }
        )
        for mapping in data.source_face_map:
            observation = observations[data.id, mapping.face_id]
            src = observation.card.image_src[mapping.source_index]
            url = image_url(src, observation.card.source.url)
            item = by_url.get((data.region, url))
            image_id = (
                "img:binding:"
                + digest(
                    canonical(
                        {
                            "source_id": observation.card.source.id
                            if item is None
                            else item.source.id,
                            "src": src,
                        }
                    )
                )[7:]
            )
            assets[image_id] = {
                "id": image_id,
                "source_src_raw": src,
                "source_url": url,
                "width": None if item is None else item.result.source_width,
                "height": None if item is None else item.result.source_height,
                "format": None if item is None else "png",
            }
            rows.append(
                {
                    "printing_id": data.id,
                    "face_id": mapping.face_id,
                    "image_id": image_id,
                    "publication_state": "pending" if item is None else "approved",
                    "availability": "unfetched" if item is None else "available",
                }
            )
            bindings.append(
                ImageBinding(
                    printing_id=data.id,
                    face_id=mapping.face_id,
                    int_id=allocations[data.id],
                    ordinal=ordinals[mapping.face_id],
                )
            )
            if item is not None:
                crop_bindings.append(
                    CropBinding(
                        data.card_id,
                        data.id,
                        mapping.face_id,
                        data.region,
                        data.card_no,
                        url,
                    )
                )
                for variant in item.result.variants:
                    variants[image_id, variant.size_key] = {
                        "image_id": image_id,
                        "size_key": variant.size_key,
                        "format": variant.format,
                        "path": variant.path,
                        "width": variant.width,
                        "height": variant.height,
                        "bytes": variant.bytes,
                    }
    tables: dict[str, list[Record]] = {
        "printing": sorted(prints, key=lambda row: string(row["id"])),
        "face": [{"id": face.id, "ordinal": face.ordinal} for face in faces],
        "image_asset": [assets[key] for key in sorted(assets)],
        "printing_image": sorted(
            rows, key=lambda row: (string(row["printing_id"]), string(row["face_id"]))
        ),
        "image_variant": [variants[key] for key in sorted(variants)],
    }
    return (
        Projection(tables, {}, {}),
        tuple(sorted(bindings, key=lambda item: (item.printing_id, item.face_id))),
        tuple(sorted(crop_bindings, key=lambda item: (item.printing_id, item.face_id))),
    )


def write_images(
    plan: MediaPlan, roots: Roots, library: Path, previous: JsonValue
) -> int:
    """Write changed permanent keys; committed versions describe already verified files."""
    old = {} if previous is None else object_value(object_value(previous)["members"])
    changed: set[str] = set()
    for row in plan.projection.tables["printing_image"]:
        key = canonical([row["printing_id"], row["face_id"]]).decode()
        before = object_value(old[key]) if key in old else {}
        for group in ("card", "art"):
            if row[group + "_version"] != before.get(group + "_version"):
                changed.add(key + ":" + group)
    # Blobs are only opened when their event version changed or a destination is missing.
    prints = {string(row["id"]): row for row in plan.projection.tables["printing"]}
    faces = {string(row["id"]): row for row in plan.projection.tables["face"]}
    paths = {
        image_path(
            integer(prints[string(row["printing_id"])]["int_id"]),
            integer(faces[string(row["face_id"])]["ordinal"]),
            string(object_value(v)["size_key"]),
        )
        for row in plan.projection.tables["printing_image"]
        for v in array(row["variants"])
        if canonical([row["printing_id"], row["face_id"]]).decode()
        + ":"
        + string(object_value(v)["size_key"]).split("_", 1)[0]
        in changed
    }
    count = 0
    for asset in plan.assets:
        path = string(asset["path"])
        target = roots.destination(path)
        if (
            path not in paths
            and target.is_file()
            and target.stat().st_size == asset["bytes"]
        ):
            continue
        raw = (library / string(asset["source"])).read_bytes()
        if digest(raw) != asset["sha256"] or len(raw) != asset["bytes"]:
            raise ValueError("Image output differs from prepared metadata")
        if target.is_file() and target.read_bytes() != raw:
            roots.destination(POINTER).unlink(missing_ok=True)
        preview.write_artifact(roots, path, raw, immutable=False)
        if digest(target.read_bytes()) != asset["sha256"]:
            raise ValueError("Written image differs from source")
        count += 1
    return count


@app.command("build-index")
def build_index_command(  # ruff: ignore[too-many-arguments, too-many-positional-arguments, too-many-locals] -- the CLI owns explicit isolated roots, source readers and the complete image commit boundary
    inputs: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    image_assets_dir: Annotated[Path, typer.Option()],
    image_cache_dir: Annotated[Path, typer.Option()],
    image_output_dir: Annotated[Path, typer.Option()],
    private_dir: Annotated[Path, typer.Option()],
    workers: Annotated[int, typer.Option(min=1, max=MAX_WORKERS)] = 2,
    verify_all: Annotated[bool, typer.Option()] = False,
    initial: Annotated[
        bool, typer.Option(help="Initialize a new image version state.")
    ] = False,
) -> None:
    """Convert images and reserve event versions independently of text export."""
    recipe = Inputs.model_validate_json(inputs.read_bytes())
    library = PreviewRoots(image_assets_dir, image_cache_dir)
    roots = Roots(image_output_dir, private_dir)
    roots.verify()
    library.validate(
        (recipe.repo, recipe.archive, inputs, roots.preview, roots.private)
    )
    for output in (roots.preview, roots.private):
        for protected in (recipe.repo, recipe.archive, inputs):
            if output.resolve().is_relative_to(
                protected.resolve()
            ) or protected.resolve().is_relative_to(output.resolve()):
                raise ValueError("Image outputs overlap protected inputs")
    state_exists = roots.destination(STATE, private=True).exists()
    if initial == state_exists:
        raise ValueError(
            "--initial requires an absent media-state.json"
            if state_exists
            else "Missing media-state.json: restore image state or use --initial for a first build"
        )
    checks = ImageChecks(reuse=not verify_all)
    batches = {
        pin.region: FrozenSources(
            recipe.archive, recipe.store_id, pin.image_batch, verify_raw=verify_all
        )
        for pin in recipe.sources
    }
    crops = load_image_crops(authored_root(recipe.repo))
    parts = tuple(
        build_regional_assets(
            batches[pin.region],
            library,
            region=pin.region,
            crops=crops,
            workers=workers,
            checks=checks,
        )
        for pin in recipe.sources
    )
    images = ImageBuild(
        tuple(item for part in parts for item in part.images),
        sum(part.elapsed_seconds for part in parts),
    )
    text_batches = FrozenBatches()
    en, jp = recipe.sources
    identity = plan_preview(
        authored_root(recipe.repo),
        FrozenRegions(
            en=FrozenEN(
                recipe.archive,
                recipe.store_id,
                en.card_batch,
                parser_version=en.parser_version,
                sources=text_batches.batch(
                    recipe.archive, recipe.store_id, en.card_batch
                ),
            ),
            jp=FrozenJP(
                recipe.archive,
                recipe.store_id,
                jp.card_batch,
                parser_version=jp.parser_version,
                sources=text_batches.batch(
                    recipe.archive, recipe.store_id, jp.card_batch
                ),
            ),
        ),
        regions=("en", "jp"),
    )
    texts = TextObservations(
        identity,
        RegionalTexts(
            {
                pin.region: FrozenTexts(
                    recipe.archive,
                    recipe.store_id,
                    pin.card_batch,
                    region=pin.region,
                    parser_version=pin.parser_version,
                    sources=text_batches.batch(
                        recipe.archive, recipe.store_id, pin.card_batch
                    ),
                )
                for pin in recipe.sources
            }
        ),
        images=RegionalImages(recipe, batches, verify_all=verify_all),
    ).plan
    projection, bindings, crop_bindings = media_projection(texts, images)
    diagnostics = crop_report(crops, images, crop_bindings)
    revision, previous = reserve(roots)
    plan = prepare_media(
        projection,
        library.preview,
        revision=revision,
        previous=previous,
        checks=checks,
        verify_blobs=verify_all,
    )
    evidence = {
        (source.archive.batch_id, source.id): ImageEvidence(
            region=next(
                pin.region
                for pin in recipe.sources
                if pin.image_batch == source.archive.batch_id
            ),
            source=source,
        )
        for application in texts.corrections or ()
        for source in application.images
    }
    index = ImageIndex(
        recipe=DEFAULT_RECIPE.version,
        store_id=recipe.store_id,
        pins=tuple(
            ImagePin(region=pin.region, batch_id=pin.image_batch)
            for pin in recipe.sources
        ),
        evidence=tuple(evidence[key] for key in sorted(evidence)),
        bindings=bindings,
        tables={key: plan.projection.tables[key] for key in MEDIA_TABLES},
    )
    index.validate_tables()
    written = write_images(plan, roots, library.preview, previous)
    content = index.content()
    path = "image-index/" + digest(content)[7:] + ".json"
    preview.write_artifact(roots, path, content, immutable=True, private=True)
    preview.write_artifact(
        roots,
        "image-report.json",
        canonical({"image_index": path, "crops": diagnostics}),
        immutable=False,
        private=True,
    )
    commit(roots, plan.state)
    typer.echo(
        canonical(
            {
                "image_index": str(roots.destination(path, private=True)),
                "written_images": written,
                "crops": {
                    "applied_source_images": diagnostics["applied_source_images"],
                    **{
                        key: len(array(diagnostics[key]))
                        for key in (
                            "unused",
                            "annotation_mismatches",
                            "reprint_candidates",
                        )
                    },
                },
                "image_execution": dict[str, JsonValue](
                    images.execution() | {"workers": workers}
                ),
            }
        ).decode()
    )
