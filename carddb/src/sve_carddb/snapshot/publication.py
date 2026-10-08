"""Shared publication preflight, before a publisher acquires any output access."""

from pydantic import JsonValue

from sve_carddb.core.json import string
from sve_carddb.snapshot.contract import validate


def require_formal(manifest: dict[str, JsonValue]) -> None:
    """Refuse preview artifacts before the future formal release gates (#34)."""
    validate("Manifest", manifest, string(manifest["format_version"]))
    if string(manifest["data_version"]).startswith("preview-"):
        raise ValueError("Formal publish refuses preview artifacts")


def require_preview(
    manifest: dict[str, JsonValue], *, regions: tuple[str, ...] = ("jp",)
) -> None:
    """Keep the isolated preview writer from accepting formal or regional releases."""
    validate("Manifest", manifest, string(manifest["format_version"]))
    if not string(manifest["data_version"]).startswith("preview-"):
        raise ValueError("Preview requires a preview- data version")
    if regions not in {("jp",), ("en", "jp")}:
        raise ValueError("Unsupported preview recipe regions")
    if manifest["regions"] != list(regions):
        raise ValueError(
            "Preview requires exactly the JP region"
            if regions == ("jp",)
            else "Offline preview requires exactly EN and JP regions"
        )
