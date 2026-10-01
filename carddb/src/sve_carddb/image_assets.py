"""Convert sealed JP images and compose their verified bindings into build bundles."""

from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from io import BytesIO
from pathlib import Path, PurePosixPath
from time import perf_counter
from typing import TYPE_CHECKING

from PIL import Image, ImageOps
from pydantic import JsonValue

from sve_carddb.build_bundle import publish_bundle
from sve_carddb.build_inputs import (
    InputRecord,
    Source,
    SourceUse,
    input_record,
    insert_raw_sources,
    uses_sorted,
)
from sve_carddb.extract.official_jp import extract_card
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.image_variants import (
    DEFAULT_RECIPE,
    SIZES,
    ImageSource,
    VariantSet,
    build_variants,
)
from sve_carddb.registry.records import PrintingData
from sve_carddb.snapshot.values import canonical, digest, parse
from sve_carddb.sources.official_jp import card_url, image_url
from sve_carddb.store import resolve_within

if TYPE_CHECKING:
    from collections.abc import Callable, Iterable, Mapping

    from sve_carddb.build_db import CompiledSchema, Database, Row, Value
    from sve_carddb.build_inputs import BuildContext
    from sve_carddb.registry.preview import PreviewPlan

PARSER = "jp-image-links-v1"
MAX_WORKERS = 4


@dataclass(frozen=True)
class ImageReference:
    printing_id: str
    face_id: str
    card_no: str
    page: Source
    source_src_raw: str
    source_url: str


@dataclass(frozen=True)
class PreviewRoots:
    preview: Path
    cdn: Path
    cache: Path

    def validate(self, archives: Iterable[Path]) -> None:
        """Resolve aliases before rejecting public, private and source root overlap."""
        roots = (self.preview, self.cdn, self.cache, *archives)
        if any(not root.is_absolute() for root in roots):
            raise ValueError("Image roots must be absolute paths")
        if any(root.is_symlink() for root in roots):
            raise ValueError("Image roots must not be symlinks")
        resolved = tuple(root.resolve() for root in roots)
        for index, root in enumerate(resolved[:3]):
            if any(
                root.is_relative_to(other) or other.is_relative_to(root)
                for other in resolved[index + 1 :]
            ):
                raise ValueError(
                    "Preview, CDN, cache and archive roots must not overlap"
                )
        for root in (self.preview, self.cache):
            if root.exists() and any(path.is_symlink() for path in root.rglob("*")):
                raise ValueError("Image output roots must not contain symlinks")


@dataclass(frozen=True)
class EncodedImage:
    source: Source
    raw_bytes: int
    result: VariantSet


@dataclass(frozen=True)
class ImageBuild:
    images: tuple[EncodedImage, ...]
    elapsed_seconds: float

    def source_uses(self) -> tuple[SourceUse, ...]:
        """Declare every source actually converted, including images without bindings."""
        return uses_sorted(
            SourceUse(
                source=item.source,
                usage="jp_image_variant",
                locator=canonical({"image_id": item.result.image_id}).decode(),
            )
            for item in self.images
        )

    def report(
        self, references: tuple[ImageReference, ...] = ()
    ) -> dict[str, JsonValue]:
        """Measure converted files separately from mapped DB rows; omit card text."""
        blobs = {
            variant.path: variant.bytes
            for item in self.images
            for variant in item.result.variants
        }
        urls = {item.source.url for item in self.images}
        used = {ref.source_url for ref in references}
        return {
            "source_images": len(self.images),
            "converted_variant_rows": sum(
                len(item.result.variants) for item in self.images
            ),
            "unique_webp_files": len(blobs),
            "unique_webp_bytes": sum(blobs.values()),
            "cache_hits": sum(item.result.cache_hit for item in self.images),
            "elapsed_milliseconds": round(self.elapsed_seconds * 1000),
            "printing_faces": len(references),
            "mapped_printing_faces": sum(ref.source_url in urls for ref in references),
            "missing": [
                {"card_no": ref.card_no, "page_sha256": ref.page.sha256}
                for ref in references
                if ref.source_url not in urls
            ],
            "unmapped_image_hashes": list[JsonValue](
                sorted(
                    {
                        item.source.sha256
                        for item in self.images
                        if item.source.url not in used
                    }
                )
            ),
        }


def _text(row: Row, name: str) -> str:
    value = row.values[name]
    if not isinstance(value, str):
        raise TypeError("Image input column must contain text")
    return value


def plan_jp_images(
    db: Database, plan: PreviewPlan, cards: FrozenSources
) -> tuple[ImageReference, ...]:
    """Use the adopted source face map and the page's actual img src for each JP face."""
    db.verify()
    printing_rows = {
        row.values["id"]: row
        for row in db.rows("printing")
        if row.values["region"] == "jp"
    }
    printings = {
        record.data.id: record.data
        for record in plan.included("printing")
        if isinstance(record.data, PrintingData) and record.data.region == "jp"
    }
    if printing_rows.keys() != printings.keys():
        raise ValueError("Image identity plan does not match the build printings")
    source_rows = {row.values["id"]: row.values for row in db.rows("source_record")}
    face_keys = {
        (row.values["printing_id"], row.values["face_id"]): row
        for row in db.rows("printing_face")
    }
    references: list[ImageReference] = []
    for printing_id, data in sorted(printings.items()):
        row = printing_rows[printing_id]
        evidence = plan.evidence["jp", data.card_no]
        source, raw, descriptor = cards.read(evidence.source.id, parser_version=PARSER)
        if (
            (descriptor.provider, descriptor.kind, descriptor.url, source.kind)
            != ("jp", "card", card_url(data.card_no), "official_page")
            or source.values() != evidence.source.values()
            or source_rows.get(source.id) != source.values()
        ):
            raise ValueError("JP image page provenance differs from the identity input")
        if (row.values["source_id"], row.values["card_no"], row.values["card_id"]) != (
            source.id,
            data.card_no,
            data.card_id,
        ):
            raise ValueError("JP printing identity differs from the image page")
        record = extract_card(raw, number=data.card_no)
        mappings = data.source_face_map
        if {mapping.source_index for mapping in mappings} != set(
            range(len(record.faces))
        ) or len(mappings) != len(record.faces):
            raise ValueError(
                "Image source face map must cover every extracted face once"
            )
        expected_faces = {(printing_id, mapping.face_id) for mapping in mappings}
        if expected_faces != {key for key in face_keys if key[0] == printing_id}:
            raise ValueError("Image source face map differs from the build faces")
        for mapping in mappings:
            face_row = face_keys[printing_id, mapping.face_id]
            if face_row.values["source_id"] != source.id:
                raise ValueError("Printing face and image page provenance disagree")
            raw_src = record.faces[mapping.source_index].image
            references.append(
                ImageReference(
                    printing_id,
                    mapping.face_id,
                    data.card_no,
                    source,
                    raw_src,
                    image_url(raw_src, source.url),
                )
            )
    return tuple(sorted(references, key=lambda ref: (ref.printing_id, ref.face_id)))


def reference_uses(references: tuple[ImageReference, ...]) -> tuple[SourceUse, ...]:
    """Retain the exact page/parser use that proved each image binding."""
    return uses_sorted(
        SourceUse(
            source=ref.page,
            usage="jp_image_link",
            locator=canonical(
                {"printing_id": ref.printing_id, "face_id": ref.face_id}
            ).decode(),
        )
        for ref in references
    )


def build_jp_assets(
    images: FrozenSources, roots: PreviewRoots, *, workers: int = 1
) -> ImageBuild:
    """Convert every current JP image before any DB or public manifest is written."""
    roots.validate((images.root,))
    if type(workers) is not int or not 1 <= workers <= MAX_WORKERS:
        raise ValueError("Image worker count must be between 1 and 4")
    if {(scope.provider, scope.kind) for scope in images.inventory.scope} != {
        ("jp", "image")
    }:
        raise ValueError("Image conversion requires an exclusively JP image batch")
    start = perf_counter()

    def convert(version: str) -> EncodedImage:
        source, raw, descriptor = images.read(
            version, parser_version=DEFAULT_RECIPE.version
        )
        if (descriptor.provider, descriptor.kind, source.kind) != (
            "jp",
            "image",
            "image",
        ):
            raise ValueError("JP image batch contains another provider or source kind")
        image_id = "img:v1:" + digest(canonical({"source_id": source.id})).removeprefix(
            "sha256:"
        )
        # Conversion only knows the resource URL; HTML src enters the DB from verified bindings.
        result = build_variants(
            ImageSource(
                image_id,
                raw,
                source.sha256.removeprefix("sha256:"),
                source.url,
                "sve_card",
                "official",
                "approved",
                "available",
            ),
            blob_root=roots.preview,
            cache_root=roots.cache,
        )
        return EncodedImage(source, descriptor.raw_bytes, result)

    versions = tuple(
        sorted(item.source_version_id for item in images.inventory.current)
    )
    if workers == 1:
        results = tuple(map(convert, versions))
    else:
        with ThreadPoolExecutor(max_workers=workers) as executor:
            results = tuple(executor.map(convert, versions, buffersize=workers))
    result = ImageBuild(results, perf_counter() - start)
    verify_assets(result, roots.preview)
    return result


def verify_assets(build: ImageBuild, preview: Path) -> None:
    """Recheck five-size closure, provenance and actual decoded blob metadata."""
    ids = [item.result.image_id for item in build.images]
    urls = [item.source.url for item in build.images]
    if len(set(ids)) != len(ids) or len(set(urls)) != len(urls):
        raise ValueError("Image asset identities must be unique")
    for item in build.images:
        result = item.result
        expected_id = "img:v1:" + digest(
            canonical({"source_id": item.source.id})
        ).removeprefix("sha256:")
        if (
            result.image_id != expected_id
            or item.source.kind != "image"
            or item.source.parser_version != DEFAULT_RECIPE.version
            or result.recipe_version != DEFAULT_RECIPE.version
            or [v.size_key for v in result.variants] != [size.key for size in SIZES]
        ):
            raise ValueError("Image source, recipe or five-size closure mismatch")
        if (result.source_src_raw, "sha256:" + result.source_sha256) != (
            item.source.url,
            item.source.sha256,
        ) or min(item.raw_bytes, result.source_width, result.source_height) <= 0:
            raise ValueError("Image source metadata mismatch")
        for variant in result.variants:
            if (
                variant.image_id != result.image_id
                or not _public_variant(variant.format, variant.is_original)
                or variant.recipe_version != result.recipe_version
            ):
                raise ValueError("Image variant identity or recipe mismatch")
            expected_path = f"images/sha256/{variant.sha256[:2]}/{variant.sha256}.webp"
            if variant.path != expected_path:
                raise ValueError("Image variant path is not content addressed")
            data = resolve_within(preview, PurePosixPath(variant.path)).read_bytes()
            if len(data) != variant.bytes or digest(data) != "sha256:" + variant.sha256:
                raise ValueError("Image variant blob hash or bytes mismatch")
            with Image.open(BytesIO(data)) as decoded:
                if decoded.format != "WEBP" or decoded.size != (
                    variant.width,
                    variant.height,
                ):
                    raise ValueError(
                        "Image variant decoded dimensions or format mismatch"
                    )


def _public_variant(format_name: str, is_original: bool) -> bool:
    return format_name == "webp" and not is_original


def populate_jp_assets(
    db: Database,
    build: ImageBuild,
    references: tuple[ImageReference, ...],
    preview: Path,
) -> tuple[SourceUse, ...]:
    """Populate only verified bindings in the caller's bundle transaction after blobs."""
    verify_assets(build, preview)
    if len({(ref.printing_id, ref.face_id) for ref in references}) != len(references):
        raise ValueError("Duplicate printing image binding")
    printing_rows = {row.values["id"]: row for row in db.rows("printing")}
    face_rows = {
        (row.values["printing_id"], row.values["face_id"]): row
        for row in db.rows("printing_face")
    }
    for ref in references:
        if (
            ref.page.kind != "official_page"
            or ref.page.parser_version != PARSER
            or ref.source_url != image_url(ref.source_src_raw, ref.page.url)
            or not ref.source_src_raw
        ):
            raise ValueError("Invalid official image binding provenance")
        printing = printing_rows.get(ref.printing_id)
        face = face_rows.get((ref.printing_id, ref.face_id))
        if printing is None or face is None:
            raise ValueError("Image binding has no printing face")
        if (
            printing.values["region"],
            printing.values["card_no"],
            printing.values["source_id"],
            face.values["source_id"],
        ) != ("jp", ref.card_no, ref.page.id, ref.page.id):
            raise ValueError("Image binding differs from the printing page")
    uses = uses_sorted((*build.source_uses(), *reference_uses(references)))
    insert_raw_sources(db, (use.source for use in uses))
    by_url = {item.source.url: item for item in build.images}
    for size in SIZES:
        db.insert(
            "image_size",
            {
                "key": size.key,
                "purpose": size.purpose,
                "max_width": size.max_width,
                "max_height": size.max_height,
                "is_original": False,
            },
        )
    used: set[str] = set()
    for ref in references:
        item = by_url.get(ref.source_url)
        image_id = "img:binding:" + digest(
            canonical(
                {
                    "source_id": ref.page.id if item is None else item.source.id,
                    "src": ref.source_src_raw,
                }
            )
        ).removeprefix("sha256:")
        if image_id not in used:
            db.insert("image_asset", _asset(ref, item, image_id))
            if item is not None:
                _variants(db, item.result, image_id)
            used.add(image_id)
        db.insert(
            "printing_image",
            {
                "printing_id": ref.printing_id,
                "face_id": ref.face_id,
                "image_id": image_id,
            },
        )
    return uses


def _asset(
    ref: ImageReference, item: EncodedImage | None, image_id: str
) -> dict[str, Value]:
    return {
        "id": image_id,
        "origin": "official",
        "publication_state": "pending" if item is None else "approved",
        "withdrawal_reason": None,
        "review_decision_id": None,
        "source_id": ref.page.id if item is None else item.source.id,
        "source_url": ref.source_url,
        "source_src_raw": ref.source_src_raw,
        "content_hash": None if item is None else item.source.sha256,
        "mime": None if item is None else "image/png",
        "width": None if item is None else item.result.source_width,
        "height": None if item is None else item.result.source_height,
        "bytes": None if item is None else item.raw_bytes,
        "availability": "unfetched" if item is None else "available",
    }


def _variants(db: Database, result: VariantSet, image_id: str) -> None:
    for variant in result.variants:
        db.insert(
            "image_variant",
            {
                "image_id": image_id,
                "size_key": variant.size_key,
                "format": variant.format,
                "path": variant.path,
                "width": variant.width,
                "height": variant.height,
                "bytes": variant.bytes,
                "sha256": "sha256:" + variant.sha256,
                "recipe_version": variant.recipe_version,
            },
        )


def verify_asset_sources(build: ImageBuild, stores: Mapping[str, Path]) -> None:
    """Compare retained source size and oriented dimensions with the sealed PNGs."""
    batches: dict[tuple[str, str], FrozenSources] = {}
    for item in build.images:
        pin = item.source.archive
        key = pin.store_id, pin.batch_id
        if key not in batches:
            root = stores.get(pin.store_id)
            if root is None:
                raise ValueError("Image source store is not configured")
            batches[key] = FrozenSources(root, *key)
        source, raw, descriptor = batches[key].read(
            item.source.id, parser_version=DEFAULT_RECIPE.version
        )
        with Image.open(BytesIO(raw)) as opened:
            oriented = ImageOps.exif_transpose(opened)
            dimensions = oriented.size
            source_format = opened.format
        if (
            source != item.source
            or descriptor.raw_bytes != item.raw_bytes
            or source_format != "PNG"
            or dimensions != (item.result.source_width, item.result.source_height)
        ):
            raise ValueError("Image source bytes or oriented dimensions mismatch")


def publish_jp_image_bundle(  # ruff: ignore[too-many-arguments, too-many-positional-arguments] -- bind schema, inputs, assets, destinations and the caller-owned parent transaction
    schema: CompiledSchema,
    destination: Path,
    context: BuildContext,
    populate_parents: Callable[[Database], InputRecord],
    build: ImageBuild,
    references: tuple[ImageReference, ...],
    roots: PreviewRoots,
    *,
    parent_uses: tuple[SourceUse, ...],
    stores: Mapping[str, Path],
) -> InputRecord:
    """Save a complete DB/input/report seal after immutable image assets are verified."""
    roots.validate(stores.values())
    if not destination.is_absolute() or any(
        destination.resolve().is_relative_to(root.resolve())
        or root.resolve().is_relative_to(destination.resolve())
        for root in (roots.preview, roots.cdn, roots.cache, *stores.values())
    ):
        raise ValueError(
            "Private image build bundle must be isolated from asset and source roots"
        )
    verify_asset_sources(build, stores)
    config = parse(context.configuration.encode())
    if (
        not isinstance(config, dict)
        or config.get("image_recipe") != DEFAULT_RECIPE.version
    ):
        raise ValueError("Build context must pin the exact image recipe")
    expected = uses_sorted(
        (*parent_uses, *build.source_uses(), *reference_uses(references))
    )

    def populate(db: Database) -> InputRecord:
        parent_record = populate_parents(db)
        parent_record.verify(db, context, parent_uses)
        uses = populate_jp_assets(db, build, references, roots.preview)
        return input_record(context, (*parent_record.uses, *uses))

    return publish_bundle(
        schema,
        destination,
        context,
        expected,
        populate,
        build.report(references),
        stores=stores,
    )
