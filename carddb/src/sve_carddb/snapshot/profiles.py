"""Explicit wire profiles; callers cannot tune bucket counts or band widths."""

from dataclasses import dataclass

LEGACY = "1.0.0"
SHARDED = "1.1.0"


@dataclass(frozen=True)
class Profile:
    version: str
    resource: str
    buckets: int
    capabilities: tuple[str, ...]

    def width(self, role: str, partition: str, kind: str, owner: str) -> int:
        """Keep physical membership stable across data versions, including exceptions."""
        if self.version == LEGACY:
            return 1
        if role == "images":
            return 1 if kind == "global" else 64
        if role == "bootstrap":
            return 8 if kind == "global" else 32 if owner in {"BP01", "CP04"} else 64
        return 2 if kind == "global" and partition == "detail" else 32


PROFILES = (
    Profile(LEGACY, "v1", 1, ("column-partition-v1", "fragment-container-v1")),
    Profile(
        SHARDED,
        "v1_1",
        64,
        (
            "column-partition-v1",
            "fragment-container-v1",
            "image-entity-buckets-v1",
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
