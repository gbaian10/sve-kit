"""Resolve authored image evidence to exact immutable source versions."""

from typing import TYPE_CHECKING, Protocol

from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.parse.pages import official_en, official_jp

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.provenance import Source
    from sve_carddb.registry.records import CorrectionEvidence


def evidence_url(evidence: CorrectionEvidence) -> str:
    """Resolve the preserved img src against its actual regional official page."""
    return official_jp.image_url(
        evidence.image_src,
        (official_jp if evidence.region == "jp" else official_en).CARD_DIR,
    )


class ImageProvider(Protocol):
    def image(self, evidence: CorrectionEvidence) -> Source:
        """Verify bytes, regional identity, URL and metadata before returning a pin."""
        ...


class FrozenImages:
    def __init__(
        self,
        root: Path,
        store_id: str,
        batch_id: str,
        *,
        sources: FrozenSources | None = None,
    ) -> None:
        self.sources = sources or FrozenSources(root, store_id, batch_id)
        if (
            self.sources.root.resolve(),
            self.sources.store_id,
            self.sources.batch_id,
        ) != (root.resolve(), store_id, batch_id):
            raise ValueError(
                "Correction image source batch differs from configured input"
            )
        self.versions: dict[tuple[str, str, str], str] = {}
        for entry in self.sources.inventory.entries:
            descriptor = self.sources.descriptor(entry.source_version_id)
            if descriptor.kind == "image":
                self.versions[
                    descriptor.provider, descriptor.url, descriptor.raw_sha256
                ] = descriptor.id

    def image(self, evidence: CorrectionEvidence) -> Source:
        """Select by exact evidence hash, including historical images, never latest."""
        url = evidence_url(evidence)
        version = self.versions.get((evidence.region, url, evidence.sha256))
        if version is None:
            raise ValueError("Correction image evidence is absent from pinned batch")
        source, _raw, descriptor = self.sources.read(
            version, parser_version="correction-image-evidence-v1"
        )
        if (
            descriptor.provider,
            descriptor.kind,
            source.kind,
            source.url,
            source.sha256,
        ) != (evidence.region, "image", "image", url, evidence.sha256):
            raise ValueError("Correction image source identity mismatch")
        return source
