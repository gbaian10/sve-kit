"""Explicit wire profiles; callers cannot tune bucket counts or band widths."""

from dataclasses import dataclass

MEDIA = "4.0.0"


@dataclass(frozen=True)
class Bands:
    family: int
    shared: int
    overrides: tuple[tuple[str, int], ...] = ()

    def width(self, family: str) -> int:
        """Resolve the fixed width without deriving overrides from snapshot contents."""
        for name, width in self.overrides:
            if name == family:
                return width
        return self.family if family else self.shared


@dataclass(frozen=True)
class Profile:
    version: str
    resource: str
    buckets: int
    capabilities: tuple[str, ...]
    min_reader_version: str
    text_bands: Bands
    image_bands: Bands

    def width(self, purpose: str, family: str) -> int:
        """Keep physical membership stable across data versions, including exceptions."""
        return (self.image_bands if purpose == "images" else self.text_bands).width(
            family
        )


PROFILES = (
    Profile(
        MEDIA,
        "v3",
        64,
        (
            "column-partition-v1",
            "digital-same-name-links-v1",
            "fragment-container-v1",
            "image-entity-buckets-v1",
            "image-id-url-v1",
            "jp-source-translation-v1",
            "public-annotation-v1",
            "rules-name-on-demand-v1",
        ),
        min_reader_version=MEDIA,
        text_bands=Bands(family=64, shared=8, overrides=(("BP01", 32), ("CP04", 32))),
        image_bands=Bands(family=32, shared=1),
    ),
)


def profile(version: str) -> Profile:
    """Reject unknown minor versions instead of inheriting the nearest profile."""
    for item in PROFILES:
        if version == item.version:
            return item
    raise ValueError("Unsupported snapshot format profile")
