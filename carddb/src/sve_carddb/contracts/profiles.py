"""Explicit wire profiles; callers cannot tune bucket counts or band widths."""

from dataclasses import dataclass

MEDIA = "4.0.0"


@dataclass(frozen=True)
class Profile:
    version: str
    resource: str
    buckets: int
    capabilities: tuple[str, ...]

    @staticmethod
    def width(purpose: str, family: str) -> int:
        """Keep physical membership stable across data versions, including exceptions."""
        if purpose == "images":
            return 32 if family else 1
        if purpose == "dictionary":
            return 2
        return 32 if family in {"BP01", "CP04"} else 64 if family else 8


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
    ),
)


def profile(version: str) -> Profile:
    """Reject unknown minor versions instead of inheriting the nearest profile."""
    for item in PROFILES:
        if version == item.version:
            return item
    raise ValueError("Unsupported snapshot format profile")
