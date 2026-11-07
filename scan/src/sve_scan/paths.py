"""Where the scanner reads and writes. Every location comes from the environment, so nothing machine-specific
lives in the repo (see scan/README.md for the expected layout of each root)."""

import os
from pathlib import Path


def _root(var: str) -> Path:
    value = os.environ.get(var)
    if not value:
        raise SystemExit(f"{var} is not set (see scan/README.md)")
    return Path(value)


def work() -> Path:
    """Generated files: gallery, banks, checkpoints, ONNX models, indexes, logs (SVE_SCAN_WORK)."""
    path = _root("SVE_SCAN_WORK")
    path.mkdir(parents=True, exist_ok=True)
    return path


def data() -> Path:
    """Private photos and the material derived from them; never published (SVE_SCAN_DATA)."""
    return _root("SVE_SCAN_DATA")


def export() -> Path:
    """A public export root: snapshots/ plus images/<size>/<n>.webp (SVE_SCAN_EXPORT)."""
    return _root("SVE_SCAN_EXPORT")


def card_images(size: str = "card_l") -> Path:
    return export() / "images" / size


def real() -> Path:
    """Labelled real photos, one folder per source with a manifest.jsonl."""
    return data() / "real"


def deck_photos() -> Path:
    """Deck-spread photos from social posts and their SIFT-checked labels."""
    return data() / "web-x" / "sample-events-orig"


def glare_bank() -> Path:
    return data() / "web-x" / "glare-bank"


def det_photos() -> Path:
    """Openly licensed photo sets used as real detector training data."""
    return data() / "web-det"
