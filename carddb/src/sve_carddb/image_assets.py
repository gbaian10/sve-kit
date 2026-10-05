"""Convert sealed regional images and compose their verified bindings into build bundles."""

from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from io import BytesIO
from pathlib import Path, PurePosixPath
from time import perf_counter
from typing import TYPE_CHECKING

from PIL import Image, ImageOps
from pydantic import JsonValue

from sve_carddb.build_inputs import Source, SourceUse, insert_raw_sources, uses_sorted
from sve_carddb.extract.official_en import extract_card as extract_en
from sve_carddb.extract.official_jp import extract_card
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.image_variants import (
    DEFAULT_RECIPE,
    SIZES,
    ImageSource,
    VariantSet,
    build_variants,
    crop_box,
)
from sve_carddb.registry.records import PrintingData, Region
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.sources import official_en
from sve_carddb.sources.official_jp import card_url, image_url
from sve_carddb.store import resolve_within

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping

    from sve_carddb.build_db import Database, Row, Value
    from sve_carddb.image_crops import ImageCrops
    from sve_carddb.registry.preview import PreviewPlan

PARSER = "jp-image-links-v1"
EN_PARSER = "en-image-links-v1"
PARSERS = {"jp": PARSER, "en": EN_PARSER}
MAX_WORKERS = 4


def conversion_image_id(source_version_id: str) -> str:
    """Keep conversion IDs distinct from the later HTML binding IDs."""
    return "img:v1:" + digest(canonical({"source_id": source_version_id}))[7:]


@dataclass(frozen=True)
class ImageReference:
    printing_id: str
    face_id: str
    card_no: str
    page: Source
    source_src_raw: str
    source_url: str
    region: Region


@dataclass(frozen=True)
class PreviewRoots:
    preview: Path
    cache: Path

    def validate(self, protected: Iterable[Path]) -> None:
        """Resolve aliases before rejecting output, cache and protected input overlap."""
        roots = (self.preview, self.cache, *protected)
        if any(not root.is_absolute() for root in roots):
            raise ValueError("Image roots must be absolute paths")
        if any(root.is_symlink() for root in roots):
            raise ValueError("Image roots must not be symlinks")
        resolved = tuple(root.resolve() for root in roots)
        for index, root in enumerate(resolved[:2]):
            if any(
                root.is_relative_to(other) or other.is_relative_to(root)
                for other in resolved[index + 1 :]
            ):
                raise ValueError("Image output, cache and input roots must not overlap")
        for root in (self.preview, self.cache):
            if root.exists() and any(path.is_symlink() for path in root.rglob("*")):
                raise ValueError("Image output roots must not contain symlinks")


@dataclass(frozen=True)
class EncodedImage:
    source: Source
    raw_bytes: int
    result: VariantSet
    region: Region
    elapsed_seconds: float


@dataclass(frozen=True)
class ImageBuild:
    images: tuple[EncodedImage, ...]
    elapsed_seconds: float

    def source_uses(self) -> tuple[SourceUse, ...]:
        """Declare every source actually converted, including images without bindings."""
        return uses_sorted(
            SourceUse(
                source=item.source,
                usage=item.region + "_image_variant",
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

    def execution(self) -> dict[str, int]:
        """Measure this run only; per-image times overlap when workers run in parallel."""
        hits = [item for item in self.images if item.result.cache_hit]
        encoded = [item for item in self.images if not item.result.cache_hit]
        return {
            "wall_milliseconds": round(self.elapsed_seconds * 1000),
            "cache_hits": len(hits),
            "new_encodings": len(encoded),
            "reuse_milliseconds": round(sum(i.elapsed_seconds for i in hits) * 1000),
            "new_encoding_milliseconds": round(
                sum(i.elapsed_seconds for i in encoded) * 1000
            ),
        }


def _text(row: Row, name: str) -> str:
    value = row.values[name]
    if not isinstance(value, str):
        raise TypeError("Image input column must contain text")
    return value


def _parser(region: Region) -> str:
    if region not in PARSERS:
        raise ValueError("Unsupported image region")
    return PARSERS[region]


def _page_url(region: Region, number: str) -> str:
    return (official_en.card_url if region == "en" else card_url)(number)


def _face_images(raw: bytes, number: str, region: Region) -> tuple[str, ...]:
    record = (extract_en if region == "en" else extract_card)(raw, number=number)
    return tuple(face.image for face in record.faces)


def plan_regional_images(
    db: Database, plan: PreviewPlan, cards: FrozenSources, *, region: Region
) -> tuple[ImageReference, ...]:
    """Bind each regional page to its adopted source face map and actual img src."""
    parser = _parser(region)
    db.verify()
    printing_rows = {
        row.values["id"]: row
        for row in db.rows("printing")
        if row.values["region"] == region
    }
    printings = {
        record.data.id: record.data
        for record in plan.included("printing")
        if isinstance(record.data, PrintingData) and record.data.region == region
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
        evidence = plan.evidence[region, data.card_no]
        source, raw, descriptor = cards.read(evidence.source.id, parser_version=parser)
        if (descriptor.provider, descriptor.kind, descriptor.url, source.kind) != (
            region,
            "card",
            _page_url(region, data.card_no),
            "official_page",
        ):
            raise ValueError(
                f"{region.upper()} image descriptor differs from the card page"
            )
        if (
            source.values() != evidence.source.values()
            or source_rows.get(source.id) != source.values()
        ):
            raise ValueError(
                f"{region.upper()} image page provenance differs from the identity input"
            )
        if (row.values["source_id"], row.values["card_no"], row.values["card_id"]) != (
            source.id,
            data.card_no,
            data.card_id,
        ):
            raise ValueError(
                f"{region.upper()} printing identity differs from the image page"
            )
        face_images = _face_images(raw, data.card_no, region)
        if {mapping.source_index for mapping in data.source_face_map} != set(
            range(len(face_images))
        ) or len(data.source_face_map) != len(face_images):
            raise ValueError(
                "Image source face map must cover every extracted face once"
            )
        expected_faces = {
            (printing_id, mapping.face_id) for mapping in data.source_face_map
        }
        if expected_faces != {key for key in face_keys if key[0] == printing_id}:
            raise ValueError("Image source face map differs from the build faces")
        for mapping in data.source_face_map:
            face_row = face_keys[printing_id, mapping.face_id]
            if face_row.values["source_id"] != source.id:
                raise ValueError("Printing face and image page provenance disagree")
            raw_src = face_images[mapping.source_index]
            references.append(
                ImageReference(
                    printing_id,
                    mapping.face_id,
                    data.card_no,
                    source,
                    raw_src,
                    image_url(raw_src, source.url),
                    region,
                )
            )
    return tuple(sorted(references, key=lambda ref: (ref.printing_id, ref.face_id)))


def reference_uses(references: tuple[ImageReference, ...]) -> tuple[SourceUse, ...]:
    """Retain the exact page/parser use that proved each image binding."""
    return uses_sorted(
        SourceUse(
            source=ref.page,
            usage=ref.region + "_image_link",
            locator=canonical(
                {"printing_id": ref.printing_id, "face_id": ref.face_id}
            ).decode(),
        )
        for ref in references
    )


def build_regional_assets(
    images: FrozenSources,
    roots: PreviewRoots,
    *,
    region: Region,
    crops: ImageCrops,
    workers: int = 1,
) -> ImageBuild:
    """Convert one pinned regional image batch, reusing every verified cache hit."""
    if region not in PARSERS:
        raise ValueError("Unsupported image region")
    roots.validate((images.root,))
    if type(workers) is not int or not 1 <= workers <= MAX_WORKERS:
        raise ValueError("Image worker count must be between 1 and 4")
    if {(scope.provider, scope.kind) for scope in images.inventory.scope} != {
        (region, "image")
    }:
        raise ValueError(
            f"Image conversion requires an exclusively {region.upper()} image batch"
        )
    start = perf_counter()

    def convert(version: str) -> EncodedImage:
        began = perf_counter()
        source, raw, descriptor = images.read(
            version, parser_version=DEFAULT_RECIPE.version
        )
        if (descriptor.provider, descriptor.kind, source.kind) != (
            region,
            "image",
            "image",
        ):
            raise ValueError(
                f"{region.upper()} image batch contains another provider or source kind"
            )
        # Conversion only knows the resource URL; HTML src enters the DB from verified bindings.
        result = build_variants(
            ImageSource(
                conversion_image_id(source.id),
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
            override=crops.box(descriptor),
        )
        return EncodedImage(
            source, descriptor.raw_bytes, result, region, perf_counter() - began
        )

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
        if (
            result.image_id != conversion_image_id(item.source.id)
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


def _verify_binding(
    ref: ImageReference, printing: Row | None, face: Row | None
) -> None:
    if (
        ref.region not in PARSERS
        or ref.page.kind != "official_page"
        or ref.page.parser_version != PARSERS[ref.region]
        or ref.source_url != image_url(ref.source_src_raw, ref.page.url)
        or not ref.source_src_raw
    ):
        raise ValueError("Invalid official image binding provenance")
    if ref.page.url != _page_url(ref.region, ref.card_no):
        raise ValueError("Image binding differs from the regional page URL")
    if printing is None or face is None:
        raise ValueError("Image binding has no printing face")
    if (
        printing.values["region"],
        printing.values["card_no"],
        printing.values["source_id"],
        face.values["source_id"],
    ) != (ref.region, ref.card_no, ref.page.id, ref.page.id):
        raise ValueError("Image binding differs from the printing page")


def populate_assets(
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
        _verify_binding(
            ref,
            printing_rows.get(ref.printing_id),
            face_rows.get((ref.printing_id, ref.face_id)),
        )
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
        if item is not None and item.region != ref.region:
            raise ValueError("Image binding crosses regional image batches")
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


def verify_asset_sources(
    build: ImageBuild, stores: Mapping[str, Path], *, crops: ImageCrops | None = None
) -> None:
    """Compare retained source size and oriented dimensions with the sealed PNGs."""
    batches: dict[tuple[str, str], FrozenSources] = {}
    current: dict[tuple[str, str], set[str]] = {}
    for item in build.images:
        pin = item.source.archive
        key = pin.store_id, pin.batch_id
        if key not in batches:
            root = stores.get(pin.store_id)
            if root is None:
                raise ValueError("Image source store is not configured")
            batches[key] = FrozenSources(root, *key)
            current[key] = {
                entry.source_version_id for entry in batches[key].inventory.current
            }
        source, raw, descriptor = batches[key].read(
            item.source.id, parser_version=DEFAULT_RECIPE.version
        )
        if item.source.id not in current[key] or (
            descriptor.provider,
            descriptor.kind,
        ) != (item.region, "image"):
            raise ValueError("Image source is not current in its regional batch")
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
        expected = crop_box(
            *dimensions, None if crops is None else crops.box(descriptor)
        )
        if item.result.crop_box != expected:
            raise ValueError("Image crop box differs from adopted source crop")
