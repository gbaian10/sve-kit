"""Verify complete out-of-scope rows before borrowing any previously produced image."""

from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING, Literal, NoReturn

from pydantic import JsonValue

from sve_carddb.core.json import canonical, digest, integer, object_value, string
from sve_carddb.core.models import Hash, RecordData
from sve_carddb.domains.registry.records import PrintingData
from sve_carddb.export.image_index import ImageIndex
from sve_carddb.export.image_scope import ImageScope, ScopeBinding
from sve_carddb.export.media_urls import image_path
from sve_carddb.export.preview import write_artifact
from sve_carddb.export.read_api import (
    POINTER,
    directory,
    load_image_upload,
    read_member,
)
from sve_carddb.images.assets import EncodedImage, conversion_image_id
from sve_carddb.images.crops import CropRecord, ImageCrops
from sve_carddb.images.variants import (
    DEFAULT_RECIPE,
    SIZES,
    ImageVariant,
    VariantSet,
    crop_box,
)
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.parse.pages.official_jp import image_url

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.provenance import Source
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.text_observations.plan import TextPlan
    from sve_carddb.export.image_upload import ImageObject, ImageUpload
    from sve_carddb.export.preview import Roots
    from sve_carddb.ingest.archive.source_archive import Descriptor


class ReuseEvidence(RecordData):
    format: Literal["sve-image-reuse-v1"] = "sve-image-reuse-v1"
    image_index_sha256: Hash
    recipe: dict[str, JsonValue]
    crops: tuple[CropRecord, ...]

    def content(self) -> bytes:
        """Keep crop evidence separate from the stable image index format."""
        return canonical(self.model_dump(mode="json"))

    @classmethod
    def read(cls, path: Path, index: ImageIndex) -> ReuseEvidence:
        """Require the exact successful index and the complete current recipe."""
        directory(path.parent)
        raw = read_member(path.parent, path.name)
        result = cls.model_validate_json(raw)
        if raw != result.content() or path.name != digest(raw)[7:] + ".json":
            raise ValueError("Invalid image reuse evidence")
        if result.image_index_sha256 != digest(index.content()):
            raise ValueError("Image reuse evidence pins another image index")
        if result.recipe != DEFAULT_RECIPE.definition():
            raise ValueError("Image recipe changed; perform a full image rebuild")
        if len({row.key for row in result.crops}) != len(result.crops):
            raise ValueError("Duplicate image reuse crop")
        return result


def printing_membership(texts: TextPlan) -> dict[str, str]:
    """Scope is publication membership, including registered but unavailable images."""
    return {
        record.data.id: record.data.home_set_id
        for record in texts.publication_identity().included("printing")
        if isinstance(record.data, PrintingData)
    }


def scope_manifest(
    texts: TextPlan, index: ImageIndex, upload: ImageUpload
) -> ImageScope:
    """Attach public membership without disclosing source URLs or local paths."""
    membership = printing_membership(texts)
    rows = {
        (string(row["printing_id"]), string(row["face_id"])): row
        for row in index.tables["printing_image"]
    }
    keys: dict[str, list[str]] = {printing: [] for printing in membership}
    for binding in index.bindings:
        row = rows[binding.printing_id, binding.face_id]
        if (
            row["availability"] == "available"
            and row["publication_state"] == "approved"
        ):
            keys[binding.printing_id].extend(
                image_path(binding.int_id, binding.ordinal, size.key) for size in SIZES
            )
    result = ImageScope(
        image_upload_sha256=digest(upload.content()),
        bindings=tuple(
            ScopeBinding(
                printing_id=printing,
                home_set_id=membership[printing],
                keys=tuple(sorted(keys[printing])),
            )
            for printing in sorted(membership)
        ),
    )
    result.require_upload(upload)
    return result


@dataclass(frozen=True)
class ImageRequest:
    printing_id: str
    face_id: str
    region: Region
    url: str
    src: str


def image_requests(texts: TextPlan) -> tuple[ImageRequest, ...]:
    """Use the same source-bound observation selection as media_projection."""
    identity = texts.publication_identity()
    membership = printing_membership(texts)
    return tuple(
        ImageRequest(
            item.printing_id,
            item.face_id,
            item.region,
            image_url(item.card.image_src[item.source_index], item.card.source.url),
            item.card.image_src[item.source_index],
        )
        for item in texts.observations
        if item.printing_id in membership
        and item.card.source.id
        == identity.evidence[item.region, item.card_no].source.id
    )


@dataclass(frozen=True)
class PreviousImages:
    index: Path
    upload: Path
    evidence: Path
    images: Path


class ImageReuse:
    def __init__(
        self,
        inputs: PreviousImages,
        archive: Path,
        store_id: str,
        selected: frozenset[str],
    ) -> None:
        self.selected = selected
        directory(inputs.images)
        self.index = ImageIndex.read(inputs.index)
        if self.index.recipe != DEFAULT_RECIPE.version:
            raise ValueError("Image recipe changed; perform a full image rebuild")
        self.index.validate_tables()
        if self.index.store_id != store_id:
            raise ValueError("Previous image index uses another source store")
        self.upload = load_image_upload(inputs.upload)
        if self.upload.image_index_sha256 != digest(self.index.content()):
            raise ValueError("Previous image upload pins another image index")
        validate_handoff(self.index, self.upload)
        evidence = ReuseEvidence.read(inputs.evidence, self.index)
        self.crops = ImageCrops(
            MappingProxyType({row.key: row for row in evidence.crops})
        )
        self.image_dir = inputs.images
        self.old_sources = {
            pin.region: FrozenSources(archive, store_id, pin.batch_id, verify_raw=False)
            for pin in self.index.pins
        }
        self.files: dict[str, ImageObject] = {}
        self.rows = {
            (string(row["printing_id"]), string(row["face_id"])): row
            for row in self.index.tables["printing_image"]
        }
        self.assets = {
            string(row["id"]): row for row in self.index.tables["image_asset"]
        }
        self.variants = {
            (string(row["image_id"]), string(row["size_key"])): row
            for row in self.index.tables["image_variant"]
        }
        self.uploads = {row.key: row for row in self.upload.objects}
        self.bindings = {
            (row.printing_id, row.face_id): row for row in self.index.bindings
        }

    def images(
        self,
        requests: tuple[ImageRequest, ...],
        batches: dict[Region, FrozenSources],
        crops: ImageCrops,
    ) -> tuple[EncodedImage, ...]:
        """Hash every borrowed object, including shared sources, without decoding."""
        sources = source_metadata(batches)
        old_sources = source_metadata(self.old_sources)
        result: dict[tuple[Region, str], EncodedImage] = {}
        for request in requests:
            if request.printing_id in self.selected:
                continue
            key = request.region, request.url
            image = self.borrow(request, sources.get(key), old_sources.get(key), crops)
            if image is None:
                continue
            if key in result and result[key] != image:
                self.fail(
                    request.printing_id, "shared source has different previous variants"
                )
            result[key] = image
        return tuple(result[key] for key in sorted(result))

    def borrow(
        self,
        request: ImageRequest,
        current: tuple[Source, Descriptor] | None,
        old: tuple[Source, Descriptor] | None,
        crops: ImageCrops,
    ) -> EncodedImage | None:
        """Reconstruct conversion metadata only after comparing the two frozen inputs."""
        label = request.printing_id
        row = self.rows.get((label, request.face_id))
        if row is None:
            self.fail(label, "missing previous row")
        assert row is not None
        asset = self.assets[string(row["image_id"])]
        if (asset["source_src_raw"], asset["source_url"]) != (request.src, request.url):
            self.fail(label, "source binding changed")
        if current is None or old is None:
            if (
                current is not None
                or old is not None
                or row["availability"] != "unfetched"
            ):
                self.fail(label, "source availability changed")
            return None
        source, descriptor = current
        old_source, old_descriptor = old
        if source.sha256 != old_source.sha256 or source.id != old_source.id:
            self.fail(label, "source sha256 changed")
        if crops.box(descriptor) != self.crops.box(old_descriptor):
            self.fail(label, "crop override changed")
        expected_id = (
            "img:binding:"
            + digest(canonical({"source_id": source.id, "src": request.src}))[7:]
        )
        if asset["id"] != expected_id:
            self.fail(label, "source identity changed")
        width, height = integer(asset["width"]), integer(asset["height"])
        return EncodedImage(
            source,
            descriptor.raw_bytes,
            VariantSet(
                conversion_image_id(source.id),
                source.url,
                source.sha256[7:],
                width,
                height,
                crop_box(width, height, crops.box(descriptor)),
                DEFAULT_RECIPE.version,
                self.borrow_variants(request, source.id),
                False,
            ),
            request.region,
            0,
            reused=True,
        )

    def borrow_variants(
        self, request: ImageRequest, source_id: str
    ) -> tuple[ImageVariant, ...]:
        """A handoff pins each fixed key's digest; its index pins dimensions and versions."""
        key = request.printing_id, request.face_id
        binding = self.bindings[key]
        row = self.rows[key]
        converted = []
        for size in SIZES:
            public_key = image_path(binding.int_id, binding.ordinal, size.key)
            expected = self.uploads[public_key]
            try:
                raw = read_member(self.image_dir, public_key)
            except OSError, ValueError:
                self.fail(request.printing_id, "WebP missing or unsafe: " + public_key)
            if (digest(raw), len(raw)) != (expected.sha256, expected.bytes):
                self.fail(
                    request.printing_id, "WebP sha256 or bytes changed: " + public_key
                )
            self.files[public_key] = expected
            variant = self.variants[string(row["image_id"]), size.key]
            if variant["bytes"] != expected.bytes:
                self.fail(request.printing_id, "upload and index byte lengths differ")
            sha = expected.sha256[7:]
            converted.append(
                ImageVariant(
                    conversion_image_id(source_id),
                    size.key,
                    "webp",
                    f"images/sha256/{sha[:2]}/{sha}.webp",
                    integer(variant["width"]),
                    integer(variant["height"]),
                    expected.bytes,
                    sha,
                    DEFAULT_RECIPE.version,
                )
            )
        return tuple(converted)

    @staticmethod
    def fail(printing: str, reason: str) -> NoReturn:
        """Always identify which scope must be widened."""
        raise ValueError(f"Expand image scope to include {printing}: {reason}")

    def validate(self, index: ImageIndex) -> None:
        """Include removals and metadata-only differences in the out-of-scope check."""
        self.index.validate_tables()
        index.validate_tables()
        old = outside_rows(self.index, self.selected)
        new = outside_rows(index, self.selected)
        changed = {
            key[0] for key in old.keys() | new.keys() if old.get(key) != new.get(key)
        }
        if changed:
            raise ValueError(
                "Out-of-scope image rows changed; expand image scope: "
                + ", ".join(sorted(changed))
            )

    def validate_state(self, previous: JsonValue) -> None:
        """Do not silently reassign versions when a different producer state is supplied."""
        if previous is None:
            raise ValueError(
                "Scoped image build requires the previous committed media state"
            )
        members = object_value(object_value(previous)["members"])
        for row in self.index.tables["printing_image"]:
            if (
                row["printing_id"] in self.selected
                or row["availability"] != "available"
            ):
                continue
            key = canonical([row["printing_id"], row["face_id"]]).decode()
            if key not in members:
                self.fail(string(row["printing_id"]), "missing committed media state")
            member = object_value(members[key])
            if any(
                member.get(group + "_version") != row[group + "_version"]
                for group in ("card", "art")
            ):
                self.fail(
                    string(row["printing_id"]),
                    "committed media state differs from previous index",
                )

    def copy_images(self, roots: Roots) -> None:
        """Recheck after conversion before writing any borrowed fixed key."""
        for key, expected in self.files.items():
            raw = read_member(self.image_dir, key)
            if (digest(raw), len(raw)) != (expected.sha256, expected.bytes):
                raise ValueError("Borrowed WebP changed during image build: " + key)
            target = roots.destination(key)
            if target.resolve() != target:
                raise ValueError("Borrowed image destination must not use symlinks")
            if target.exists():
                if not target.is_file():
                    raise ValueError(
                        "Borrowed image destination must be a regular file"
                    )
                if target.read_bytes() == raw:
                    continue
                roots.destination(POINTER).unlink(missing_ok=True)
            write_artifact(roots, key, raw, immutable=False)


def source_metadata(
    batches: dict[Region, FrozenSources],
) -> dict[tuple[Region, str], tuple[Source, Descriptor]]:
    """Read descriptors and receipts, never PNG payloads."""
    return {
        (region, item.url): batch.metadata(
            item.source_version_id, parser_version=DEFAULT_RECIPE.version
        )
        for region, batch in batches.items()
        for item in batch.current_versions
    }


def validate_handoff(index: ImageIndex, upload: ImageUpload) -> None:
    """The public handoff must agree with every indexed key, version and byte length."""
    rows = {
        (string(row["printing_id"]), string(row["face_id"])): row
        for row in index.tables["printing_image"]
    }
    variants = {
        (string(row["image_id"]), string(row["size_key"])): row
        for row in index.tables["image_variant"]
    }
    expected = {}
    for binding in index.bindings:
        row = rows[binding.printing_id, binding.face_id]
        if row["availability"] != "available" or row["publication_state"] != "approved":
            continue
        for size in SIZES:
            key = image_path(binding.int_id, binding.ordinal, size.key)
            expected[key] = (
                row[size.purpose + "_version"],
                variants[string(row["image_id"]), size.key]["bytes"],
            )
    if expected != {item.key: (item.version, item.bytes) for item in upload.objects}:
        raise ValueError(
            "Previous image upload differs from indexed keys, versions or byte lengths"
        )


def outside_rows(
    index: ImageIndex, selected: frozenset[str]
) -> dict[tuple[str, str], bytes]:
    """Compare each binding and all its public rows, including shared image assets."""
    assets = {string(row["id"]): row for row in index.tables["image_asset"]}
    variants: dict[str, list[dict[str, JsonValue]]] = {}
    for row in index.tables["image_variant"]:
        variants.setdefault(string(row["image_id"]), []).append(row)
    rows = {
        (string(row["printing_id"]), string(row["face_id"])): row
        for row in index.tables["printing_image"]
    }
    return {
        (binding.printing_id, binding.face_id): canonical(
            {
                "binding": binding.model_dump(mode="json"),
                "row": rows[binding.printing_id, binding.face_id],
                "asset": assets[
                    string(rows[binding.printing_id, binding.face_id]["image_id"])
                ],
                "variants": list[JsonValue](
                    variants.get(
                        string(rows[binding.printing_id, binding.face_id]["image_id"]),
                        [],
                    )
                ),
            }
        )
        for binding in index.bindings
        if binding.printing_id not in selected
    }
