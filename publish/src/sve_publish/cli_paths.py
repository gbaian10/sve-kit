"""Validate required CLI roots after command-specific environment resolution."""

from typing import TYPE_CHECKING

import typer

if TYPE_CHECKING:
    from pathlib import Path


def required_root(value: Path | None, option: str, environment: str) -> Path:
    """Reject missing and relative roots before any input or output access."""
    if value is None:
        raise typer.BadParameter(
            f"Pass {option} or set {environment} through mise or the environment",
            param_hint=option,
        )
    if not value.is_absolute():
        raise typer.BadParameter(
            f"{environment} / {option} must be a non-empty absolute path",
            param_hint=option,
        )
    return value
