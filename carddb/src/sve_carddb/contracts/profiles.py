"""Explicit wire profiles; callers cannot tune bucket counts or band widths."""

from dataclasses import dataclass

MEDIA = "4.0.0"
PROGRAMS = "4.1.0"
PROGRAM_DEPENDENCIES = "4.2.0"


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
    global_detail_width: int

    def payload_version(self, role: str) -> str:
        """Keep existing non-program blobs in the program-dependency profile."""
        if self.version == PROGRAM_DEPENDENCIES and role != "programs":
            return PROGRAMS
        return self.version

    def width(self, purpose: str, family: str, region: str | None = None) -> int:
        """Keep physical membership stable across data versions, including exceptions."""
        if purpose == "detail" and region is None and not family:
            return self.global_detail_width
        return (self.image_bands if purpose == "images" else self.text_bands).width(
            family
        )


PROFILES: tuple[Profile, ...] = (
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
        min_reader_version="4.0.3",
        text_bands=Bands(family=64, shared=8, overrides=(("BP01", 32), ("CP04", 32))),
        image_bands=Bands(family=32, shared=1),
        global_detail_width=4,
    ),
)
PROFILES += (
    Profile(
        PROGRAMS,
        "v4",
        PROFILES[0].buckets,
        tuple(sorted((*PROFILES[0].capabilities, "dsl-programs-v1"))),
        min_reader_version="4.1.1",
        text_bands=PROFILES[0].text_bands,
        image_bands=PROFILES[0].image_bands,
        global_detail_width=PROFILES[0].global_detail_width,
    ),
)


PROFILES += (
    Profile(
        PROGRAM_DEPENDENCIES,
        "v5",
        PROFILES[1].buckets,
        tuple(sorted((*PROFILES[1].capabilities, "dsl-program-dependencies-v1"))),
        min_reader_version="4.2.0",
        text_bands=PROFILES[1].text_bands,
        image_bands=PROFILES[1].image_bands,
        global_detail_width=PROFILES[1].global_detail_width,
    ),
)


def profile(version: str) -> Profile:
    """Reject unknown minor versions instead of inheriting the nearest profile."""
    for item in PROFILES:
        if version == item.version:
            return item
    raise ValueError("Unsupported snapshot format profile")
