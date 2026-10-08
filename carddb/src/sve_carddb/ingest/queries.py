"""Read card membership only from validated manifest generations."""

from typing import TYPE_CHECKING

from sve_carddb.ingest.archive.manifest import Region

if TYPE_CHECKING:
    from sve_carddb.ingest.archive.manifest import Manifest


def sets_root(region: Region) -> str:
    """The generation root of a region's product list."""
    return f"{region.value}:sets"


SETS_ROOT = sets_root(Region.JP)


def list_root(set_code: str, region: Region = Region.JP) -> str:
    """The generation root of a product's list."""
    return f"{region.value}:list:{set_code}"


def current_sets(manifest: Manifest, region: Region = Region.JP) -> list[str]:
    """Product codes from the validated product generation."""
    current = manifest.generations.current(sets_root(region))
    if current is None:
        return []
    return [edge.link.original for edge in manifest.generations.edges(current.id)]


def card_numbers(
    manifest: Manifest,
    set_codes: list[str] | None = None,
    region: Region = Region.JP,
) -> list[str]:
    """Card numbers from validated list generations, deduplicated, in list order."""
    numbers: dict[str, None] = {}
    codes = set_codes if set_codes is not None else current_sets(manifest, region)
    for code in codes:
        current = manifest.generations.current(list_root(code, region))
        if current is None:
            continue
        for edge in manifest.generations.edges(current.id):
            numbers.setdefault(edge.link.original)
    return list(numbers)
