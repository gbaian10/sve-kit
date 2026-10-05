"""Load source-bound art crop overrides from the authored YAML file on disk."""

from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING, Annotated

from pydantic import (
    Field,
    TypeAdapter,
    ValidationError,
    field_validator,
    model_validator,
)

from sve_carddb.image_variants import CropBox
from sve_carddb.registry.inputs import JSON_VALUE
from sve_carddb.registry.records import Hash, RecordData, Region
from sve_carddb.registry.storage import MAX_BYTES
from sve_carddb.registry.yaml_reader import parse_yaml
from sve_carddb.snapshot.values import canonical, digest

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.source_archive import Descriptor

FILE = "image-crops.yaml"
HexHash = Annotated[str, Field(pattern=r"^[0-9a-f]{64}\Z")]
Nonnegative = Annotated[int, Field(ge=0)]
Positive = Annotated[int, Field(gt=0)]


def image_source_key(region: str, url: str) -> str:
    """Match the archive's resource key for an image URL."""
    return digest(canonical({"provider": region, "kind": "image", "url": url}))


class CropRecord(RecordData):
    source_key: Hash
    source_sha256: HexHash
    # Every adopted box so far is the hanged-man layout on 459x641 scans, so rows mostly differ in top.
    left: Nonnegative = 36
    top: Nonnegative
    width: Positive = 384
    height: Positive = 288
    reason: str
    region: Region
    card_no: str

    @model_validator(mode="after")
    def _ratio(self) -> CropRecord:
        if self.width * 3 != self.height * 4:
            raise ValueError("Crop must have an exact 4:3 ratio")
        return self

    @field_validator("reason", "card_no")
    @classmethod
    def _nonblank(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("Crop annotation must not be blank")
        return value

    @property
    def key(self) -> tuple[str, str]:
        """One adopted box per resource/version, independent of identity repairs."""
        return self.source_key, self.source_sha256

    def box(self) -> CropBox:
        """Return oriented integer coordinates without rotating the artwork."""
        return CropBox(self.left, self.top, self.width, self.height)


_RECORDS = TypeAdapter(tuple[CropRecord, ...])


@dataclass(frozen=True)
class ImageCrops:
    records: Mapping[tuple[str, str], CropRecord]

    def box(self, descriptor: Descriptor) -> CropBox | None:
        """A known resource with new bytes must never fall back to the standard crop."""
        key = descriptor.source_key, descriptor.raw_sha256.removeprefix("sha256:")
        record = self.records.get(key)
        if record is not None:
            return record.box()
        known = sorted(sha for source_key, sha in self.records if source_key == key[0])
        if known:
            raise ValueError(
                f"Unadopted crop source version: source_key={key[0]} new_hash={key[1]} adopted_hashes={','.join(known)}"
            )
        return None


def parse_crops(data: bytes) -> tuple[CropRecord, ...]:
    """Apply the shared strict authored YAML boundary before field validation."""
    try:
        if len(data) >= MAX_BYTES:
            raise ValueError("Oversized image crop YAML")
        raw = JSON_VALUE.validate_python(parse_yaml(data), strict=True)
        return _RECORDS.validate_json(canonical(raw))
    except ValidationError as error:
        details = "; ".join(
            ".".join(map(str, issue["loc"])) + ":" + issue["type"]
            for issue in error.errors(include_input=False, include_context=False)
        )
        raise ValueError("Invalid image crop fields: " + details) from None


def load_image_crops(authored: Path) -> ImageCrops:
    """Read the working-tree file, so an uncommitted box edit applies to the next build."""
    records: dict[tuple[str, str], CropRecord] = {}
    for record in parse_crops((authored / FILE).read_bytes()):
        if record.key in records:
            raise ValueError("Duplicate image crop key")
        records[record.key] = record
    return ImageCrops(MappingProxyType(records))
