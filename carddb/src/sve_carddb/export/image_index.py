"""Pinned image metadata consumed without opening the bitmap library."""

from dataclasses import replace
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue

from sve_carddb.core.json import array, canonical, digest, integer, object_value, string
from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.core.provenance import Source
from sve_carddb.core.regions import Region
from sve_carddb.domains.source_corrections.images import evidence_url
from sve_carddb.export.media_urls import image_path
from sve_carddb.images.variants import DEFAULT_RECIPE, SIZES
from sve_carddb.parse.pages.official_jp import image_url

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.registry.records import CorrectionEvidence
    from sve_carddb.domains.text_observations.plan import TextPlan
    from sve_carddb.export.project import Projection
    from sve_carddb.export.project.source import Record

MEDIA_TABLES = ("image_asset", "printing_image", "image_variant")


class ImagePin(RecordData):
    region: Region
    batch_id: Hash


class ImageEvidence(RecordData):
    region: Region
    source: Source


class ImageBinding(RecordData):
    printing_id: Text
    face_id: Text
    int_id: int
    ordinal: int


class ImageIndex(RecordData):
    format: Literal["sve-image-index-v1"] = "sve-image-index-v1"
    recipe: Hash
    store_id: Text
    pins: tuple[ImagePin, ...]
    evidence: tuple[ImageEvidence, ...]
    bindings: tuple[ImageBinding, ...]
    tables: dict[str, list[dict[str, JsonValue]]]

    def content(self) -> bytes:
        """Canonical bytes determine the explicit immutable input pin."""
        return canonical(self.model_dump(mode="json"))

    @classmethod
    def read(cls, path: Path) -> ImageIndex:
        """A content-addressed filename pins the producer's metadata, including versions."""
        if not path.is_absolute() or path.is_symlink():
            raise ValueError("Image index must be an absolute non-symlink file")
        raw = path.read_bytes()
        if path.name != digest(raw)[7:] + ".json":
            raise ValueError("Image index filename differs from its SHA-256")
        index = cls.model_validate_json(raw)
        if index.content() != raw:
            raise ValueError("Image index must use canonical JSON")
        return index

    def validate_tables(self) -> None:  # ruff: ignore[complex-structure] -- row contracts, identity and variant closure are independent metadata checks
        """Validate public rows and identity closure using metadata only."""
        if self.recipe != DEFAULT_RECIPE.version or set(self.tables) != set(
            MEDIA_TABLES
        ):
            raise ValueError("Image index recipe or tables differ from the contract")
        assets = {string(row["id"]): row for row in self.tables["image_asset"]}
        variants: dict[str, list[Record]] = {}
        for row in self.tables["image_variant"]:
            variants.setdefault(string(row["image_id"]), []).append(row)
        members = {
            (string(row["printing_id"]), string(row["face_id"])): row
            for row in self.tables["printing_image"]
        }
        bindings = {(item.printing_id, item.face_id): item for item in self.bindings}
        if (
            len(bindings) != len(self.bindings)
            or len(members) != len(self.tables["printing_image"])
            or members.keys() != bindings.keys()
        ):
            raise ValueError("Image index requires unique matching bindings")
        if (
            len(assets) != len(self.tables["image_asset"])
            or set(assets) != {string(row["image_id"]) for row in members.values()}
            or variants.keys() - assets.keys()
        ):
            raise ValueError("Image index asset closure mismatch")
        for key, row in members.items():
            binding = bindings[key]
            image_path(binding.int_id, binding.ordinal, "card_s")
            sizes = sorted(
                variants.get(string(row["image_id"]), []),
                key=lambda r: string(r["size_key"]),
            )
            expected = [
                {field: item[field] for field in ("size_key", "width", "height")}
                for item in sizes
            ]
            if row["variants"] != expected:
                raise ValueError("Image index display dimensions differ from variants")
            active = (
                row["availability"] == "available"
                and row["publication_state"] == "approved"
            )
            versions = (row["card_version"], row["art_version"])
            if (active and None in versions) or (
                not active and versions != (None, None)
            ):
                raise ValueError("Image index versions differ from availability")
            if active != bool(sizes):
                raise ValueError("Image index availability differs from variants")
            if active and {string(item["size_key"]) for item in sizes} != {
                size.key for size in SIZES
            }:
                raise ValueError("Image index requires all five sizes")
        if len(
            {(row["image_id"], row["size_key"]) for row in self.tables["image_variant"]}
        ) != len(self.tables["image_variant"]):
            raise ValueError("Duplicate image index variant")

    def image(self, evidence: CorrectionEvidence) -> Source:
        """Match exact regional URL/hash evidence verified by the image producer."""
        matches = [
            item.source
            for item in self.evidence
            if (item.region, item.source.url, item.source.sha256)
            == (evidence.region, evidence_url(evidence), evidence.sha256)
        ]
        if len(matches) != 1 or matches[0].kind != "image":
            raise ValueError("Correction image evidence is absent from image index")
        source = matches[0]
        pins = {item.region: item.batch_id for item in self.pins}
        if (source.archive.store_id, source.archive.batch_id) != (
            self.store_id,
            pins[evidence.region],
        ):
            raise ValueError("Correction image evidence differs from image index pins")
        return source

    def attach(self, projection: Projection, texts: TextPlan) -> Projection:
        """Check permanent IDs, region and actual parsed img src before joining media."""
        prints = {string(row["id"]): row for row in projection.tables["printing"]}
        faces = {string(row["id"]): row for row in projection.tables["face"]}
        observations = {
            (item.printing_id, item.face_id): item
            for item in texts.observations
            if item.printing_id in prints
            and item.card.source.id
            == texts.identity.evidence[item.region, item.card_no].source.id
        }
        bindings = {(item.printing_id, item.face_id): item for item in self.bindings}
        expected = {
            (string(row["id"]), string(object_value(face)["face_id"]))
            for row in prints.values()
            for face in array(row["faces"])
        }
        if bindings.keys() != expected or observations.keys() != expected:
            raise ValueError(
                "Image index printing-face membership differs from text build"
            )
        assets = {string(row["id"]): row for row in self.tables["image_asset"]}
        for row in self.tables["printing_image"]:
            key = string(row["printing_id"]), string(row["face_id"])
            binding, observation = bindings[key], observations[key]
            if (binding.int_id, binding.ordinal) != (
                integer(prints[key[0]]["int_id"]),
                integer(faces[key[1]]["ordinal"]),
            ) or prints[key[0]]["region"] != observation.region:
                raise ValueError(
                    "Image index permanent identity differs from text build"
                )
            asset = assets[string(row["image_id"])]
            if (
                len(observation.card.image_src) != len(observation.card.faces)
                or asset["source_src_raw"]
                != observation.card.image_src[observation.source_index]
            ):
                raise ValueError("Image index img src differs from parsed card page")
            if asset["source_url"] != image_url(
                string(asset["source_src_raw"]), observation.card.source.url
            ):
                raise ValueError(
                    "Image index resolved URL differs from parsed card page"
                )
        return replace(projection, tables=projection.tables | self.tables)
