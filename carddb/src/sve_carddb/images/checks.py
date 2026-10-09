"""Share image inspection results only for the lifetime of one command."""

from io import BytesIO
from threading import RLock
from typing import TYPE_CHECKING

from PIL import Image

from sve_carddb.core.json import digest
from sve_carddb.ingest.archive.frozen_sources import FrozenBatches, FrozenSources

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.provenance import Source
    from sve_carddb.images.variants import CropBox, VariantSet


class ImageChecks:
    """Bound cached results to observed files and the current conversion command."""

    def __init__(self) -> None:
        self.files: dict[
            Path, tuple[tuple[int, ...], tuple[int, str, str | None, tuple[int, int]]]
        ] = {}
        self.decoded: dict[str, tuple[str | None, tuple[int, int]]] = {}
        self.sources: set[tuple[Source, int, int, int, CropBox, str]] = set()
        self.variants: dict[
            tuple[str, str, CropBox | None, Path, Path], VariantSet
        ] = {}
        self.variant_locks: dict[
            tuple[str, str, CropBox | None, Path, Path], RLock
        ] = {}
        self.png: dict[str, tuple[str | None, tuple[int, int]]] = {}
        self.lock = RLock()
        self.batches = FrozenBatches()

    def batch(self, root: Path, store_id: str, batch_id: str) -> FrozenSources:
        """One command shares the same verified immutable batch across image stages."""
        return self.batches.batch(root, store_id, batch_id)

    def inspect(self, path: Path) -> tuple[int, str, str | None, tuple[int, int]]:
        """Changed files are read again; no previous command can supply a cache hit."""
        with self.lock:
            return self._inspect(path)

    def _inspect(self, path: Path) -> tuple[int, str, str | None, tuple[int, int]]:
        stat = path.stat()
        stamp = (
            stat.st_dev,
            stat.st_ino,
            stat.st_size,
            stat.st_mtime_ns,
            stat.st_ctime_ns,
        )
        cached = self.files.get(path)
        if cached is not None and cached[0] == stamp:
            return cached[1]
        result = self.inspect_bytes(path.read_bytes())
        self.files[path] = stamp, result
        return result

    def inspect_bytes(self, raw: bytes) -> tuple[int, str, str | None, tuple[int, int]]:
        """Hash actual bytes at a write boundary and reuse decoding by unique blob."""
        checksum = digest(raw)
        with self.lock:
            metadata = self.decoded.get(checksum)
            if metadata is None:
                with Image.open(BytesIO(raw)) as opened:
                    opened.load()
                    metadata = opened.format, opened.size
                self.decoded[checksum] = metadata
        return len(raw), checksum, *metadata
