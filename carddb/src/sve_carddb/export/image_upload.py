"""Public, source-free image upload metadata shared with publication readers."""

import re
from typing import TYPE_CHECKING, Literal, Self

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

from sve_carddb.core.json import canonical, integer, string

if TYPE_CHECKING:
    from sve_carddb.export.media import MediaPlan

IMAGE_KEY = re.compile(
    r"images/(?:card_[sml]|art_[sm])/[1-9][0-9]*(?:-f[1-9][0-9]*)?\.webp\Z"
)
HASH = r"^sha256:[0-9a-f]{64}$"


class ImageObject(BaseModel):
    """One fixed public key and the event version allocated by the producer."""

    model_config = ConfigDict(extra="forbid", strict=True, frozen=True)
    key: str
    version: int = Field(gt=0)
    sha256: str = Field(pattern=HASH)
    bytes: int = Field(gt=0)

    @field_validator("key")
    @classmethod
    def public_key(cls, value: str) -> str:
        """Allow only the five established public WebP namespaces."""
        if IMAGE_KEY.fullmatch(value) is None:
            raise ValueError("Invalid public image key")
        return value


class ImageObjects(BaseModel):
    """A bucket's successful uploads, never a claim about unlisted objects."""

    model_config = ConfigDict(extra="forbid", strict=True, frozen=True)
    objects: tuple[ImageObject, ...]

    @model_validator(mode="after")
    def unique_keys(self) -> Self:
        """Keep one row per key in canonical key order."""
        keys = [item.key for item in self.objects]
        if keys != sorted(set(keys)):
            raise ValueError("Image keys must be unique and sorted")
        return self

    def content(self) -> bytes:
        """Serialize only explicitly public fields."""
        return canonical(self.model_dump(mode="json"))


class ImageInventory(ImageObjects):
    """The successful uploads recorded in one bucket."""

    format: Literal["sve-publish-images-v1"]


class ImageUpload(ImageObjects):
    """Content-addressed producer handoff, independent of private image evidence."""

    format: Literal["sve-image-upload-v1"]
    image_index_sha256: str = Field(pattern=HASH)
    revision: int = Field(gt=0)

    @model_validator(mode="after")
    def allocated_versions(self) -> Self:
        """An event token cannot exceed its producer high-water revision."""
        if any(item.version > self.revision for item in self.objects):
            raise ValueError("Image version exceeds producer revision")
        return self


def upload_description(plan: MediaPlan, index_hash: str) -> ImageUpload:
    """Project hashes already verified during image production without reading files."""
    return ImageUpload(
        format="sve-image-upload-v1",
        image_index_sha256=index_hash,
        revision=integer(plan.state["revision"]),
        objects=tuple(
            ImageObject(
                key=string(asset["path"]),
                version=integer(asset["version"]),
                sha256=string(asset["sha256"]),
                bytes=integer(asset["bytes"]),
            )
            for asset in plan.assets
        ),
    )
