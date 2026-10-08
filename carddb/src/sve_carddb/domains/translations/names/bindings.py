"""Verified source-use bindings for translated display names."""

from dataclasses import dataclass


@dataclass(frozen=True)
class DisplayBinding:
    """Reuse an exact source use on an explicitly verified display owner."""

    source_use_id: str
    destination: tuple[str, ...]
    target_lang: str
    basis: str
    translation_id: str | None = None
