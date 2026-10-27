"""Public printing membership for selecting an image handoff without private inputs."""

from typing import Literal, Self

from pydantic import Field, model_validator

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.export.image_upload import IMAGE_KEY, ImageUpload


class ScopeBinding(RecordData):
    printing_id: Text
    home_set_id: Text
    keys: tuple[str, ...]


class ImageScope(RecordData):
    format: Literal["sve-image-scope-v1"] = "sve-image-scope-v1"
    image_upload_sha256: Hash
    bindings: tuple[ScopeBinding, ...] = Field(min_length=1)

    def content(self) -> bytes:
        """Use content addressing to pin public scope membership."""
        return canonical(self.model_dump(mode="json"))

    @model_validator(mode="after")
    def unique_members(self) -> Self:
        """Reject ambiguous membership before filtering any permanent key."""
        ids = [row.printing_id for row in self.bindings]
        keys = [key for row in self.bindings for key in row.keys]
        if ids != sorted(set(ids)) or len(keys) != len(set(keys)):
            raise ValueError("Image scope requires unique sorted printing membership")
        if any(not IMAGE_KEY.fullmatch(key) for key in keys):
            raise ValueError("Invalid image scope key")
        return self

    def select(
        self, scopes: tuple[str, ...], printings: tuple[str, ...]
    ) -> frozenset[str]:
        """Resolve exact registry names; never infer membership from card numbers."""
        selected = select_printings(
            {row.printing_id: row.home_set_id for row in self.bindings},
            scopes,
            printings,
        )
        return frozenset(
            key
            for row in self.bindings
            if row.printing_id in selected
            for key in row.keys
        )

    def require_upload(self, description: ImageUpload) -> None:
        """Membership must describe every object in this exact successful handoff."""
        if self.image_upload_sha256 != digest(description.content()) or {
            key for row in self.bindings for key in row.keys
        } != {item.key for item in description.objects}:
            raise ValueError("Image scope differs from the image upload description")


def select_printings(
    membership: dict[str, str], scopes: tuple[str, ...], printings: tuple[str, ...]
) -> frozenset[str]:
    """Use union selection while rejecting every unknown scope or printing."""
    unknown = (set(scopes) - set(membership.values())) | (
        set(printings) - membership.keys()
    )
    if unknown:
        raise ValueError(
            "Unknown image scope or unregistered printing: "
            + ", ".join(sorted(unknown))
        )
    return frozenset(
        printing
        for printing, home in membership.items()
        if home in scopes or printing in printings
    )
