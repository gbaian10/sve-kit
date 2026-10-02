"""Independent JP/EN transcription from verified sealed card sources."""

from typing import TYPE_CHECKING

from sve_carddb.extract import official_en, official_jp
from sve_carddb.extract.compare_jp import legacy_projection
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.registry.inputs import canonical
from sve_carddb.registry.records import Observation, Region
from sve_carddb.registry.review import observation
from sve_carddb.snapshot.values import digest
from sve_carddb.sources import official_en as en
from sve_carddb.sources import official_jp as jp
from sve_carddb.text_observations.models import FaceContent, TextCard
from sve_carddb.text_observations.presence import detect_presence

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path


def jp_face(face: official_jp.Face) -> FaceContent:
    """Retain each exact JP field, including absent effects and ordered sections."""
    return FaceContent(
        name=face.name,
        effect=face.text,
        sections=tuple(face.sections),
        class_raw=face.card_class,
        type_raw=face.card_type,
        stats=(face.cost, face.power, face.hp),
        traits=tuple(face.traits),
        title=face.title,
        flavor=face.flavor,
    )


def en_face(face: official_en.Face) -> FaceContent:
    """Use the new EN section transcription rather than its historical full-text shape."""
    return FaceContent(
        name=face.name,
        effect=face.text,
        sections=tuple(face.sections),
        class_raw=face.info["Class"],
        type_raw=face.info["Card Type"],
        stats=(face.stats["cost"], face.stats["power"], face.stats["hp"]),
        traits=tuple(face.traits),
        title=face.info.get("Universe"),
        flavor=face.speech,
    )


def verify_card(card: TextCard) -> None:
    """Reproduce frozen extraction and every result before accepting projected text."""
    if card.raw is None:
        if card.effect_presence:
            raise ValueError("Effect presence has no frozen source bytes")
        return
    region, number = card.observation.region, card.observation.card_no
    if digest(card.raw) != card.source.sha256:
        raise ValueError("Effect presence frozen source hash mismatch")
    faces = (
        tuple(
            jp_face(face)
            for face in official_jp.extract_card(card.raw, number=number).faces
        )
        if region == "jp"
        else tuple(
            en_face(face)
            for face in official_en.extract_card(card.raw, number=number).faces
        )
    )
    expected = tuple(
        detect_presence(
            card.raw, card.source, region=region, number=number, source_index=index
        )
        for index in range(len(faces))
    )
    if card.faces != faces or card.effect_presence != expected:
        raise ValueError("Effect presence/extraction cannot be reproduced")


class FrozenTexts:
    def __init__(
        self,
        root: Path,
        store_id: str,
        batch_id: str,
        *,
        region: Region,
        parser_version: str,
    ) -> None:
        self.sources = FrozenSources(root, store_id, batch_id)
        self.region = region
        self.parser_version = parser_version
        self.current = {
            item.url: item.source_version_id for item in self.sources.inventory.current
        }
        self._versions: dict[str, tuple[str, ...]] | None = None

    def versions(self, region: Region, card_no: str) -> tuple[str, ...]:
        """Include historical entries even when the batch's current selection changed."""
        if region != self.region:
            return ()
        url = (jp if region == "jp" else en).card_url(card_no)
        if self._versions is None:
            by_url: dict[str, list[str]] = {}
            for entry in self.sources.inventory.entries:
                descriptor = self.sources.descriptor(entry.source_version_id)
                if (descriptor.provider, descriptor.kind) == (self.region, "card"):
                    by_url.setdefault(descriptor.url, []).append(descriptor.id)
            self._versions = {
                key: tuple(sorted(values)) for key, values in by_url.items()
            }
        return self._versions.get(url, ())

    def card(self, region: Region, card_no: str) -> TextCard | None:
        """Recheck raw/version/receipt pins on each read; never visit live or latest."""
        if region != self.region:
            return None
        url = (jp if region == "jp" else en).card_url(card_no)
        version = self.current.get(url)
        if version is None:
            return None
        return self.version(region, card_no, version)

    def version(self, region: Region, card_no: str, version: str) -> TextCard:
        """Read a specified historical version inside this exact sealed batch."""
        if region != self.region:
            raise ValueError("Frozen historical text region mismatch")
        url = (jp if region == "jp" else en).card_url(card_no)
        source, raw, descriptor = self.sources.read(
            version, parser_version=self.parser_version
        )
        if (descriptor.provider, descriptor.kind, descriptor.url, source.kind) != (
            region,
            "card",
            url,
            "official_page",
        ):
            raise ValueError("Frozen text source identity/media mismatch")
        if region == "jp":
            record = official_jp.extract_card(raw, number=card_no)
            faces = tuple(jp_face(face) for face in record.faces)
            old = legacy_projection(record)
            date_raw, errata = record.release_date, record.errata_url
        else:
            english = official_en.extract_card(raw, number=card_no)
            faces = tuple(en_face(face) for face in english.faces)
            old = official_en.legacy_projection(english)
            date_raw, errata = english.release_date, english.errata_url
        return TextCard(
            source=source,
            observation=Observation.model_validate_json(
                canonical(observation(old, region))
            ),
            faces=faces,
            date_raw=date_raw,
            has_errata_link=errata is not None,
            effect_presence=tuple(
                detect_presence(
                    raw, source, region=region, number=card_no, source_index=index
                )
                for index in range(len(faces))
            ),
            raw=raw,
        )


class RegionalTexts:
    def __init__(self, providers: Mapping[Region, FrozenTexts]) -> None:
        self.providers = dict(providers)

    def card(self, region: Region, card_no: str) -> TextCard | None:
        """Dispatch only by explicit region; never infer regional identity from suffixes."""
        provider = self.providers.get(region)
        return None if provider is None else provider.card(region, card_no)
