"""Shared publication preflight, before a publisher acquires any output access."""

from pydantic import JsonValue

from sve_carddb.snapshot.contract import validate
from sve_carddb.snapshot.values import string


def require_formal(manifest: dict[str, JsonValue]) -> None:
    """Refuse preview artifacts before the future formal release gates (#34)."""
    validate("Manifest", manifest)
    if string(manifest["data_version"]).startswith("preview-"):
        raise ValueError("Formal publish refuses preview artifacts")


def require_preview(manifest: dict[str, JsonValue]) -> None:
    """Keep the isolated preview writer from accepting formal or regional releases."""
    validate("Manifest", manifest)
    if not string(manifest["data_version"]).startswith("preview-"):
        raise ValueError("Preview requires a preview- data version")
    if manifest["regions"] != ["jp"]:
        raise ValueError("Preview requires exactly the JP region")
