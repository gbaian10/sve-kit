"""Reconstruct review scope and observations from permanent inputs and frozen pages."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from pydantic import Field

from sve_carddb.core.json import canonical, parse
from sve_carddb.core.models import Date, Hash, RecordData, Text, UInt
from sve_carddb.core.provenance import FilePin, SourceUse, uses_sorted
from sve_carddb.core.regions import Region
from sve_carddb.domains import physical_sources
from sve_carddb.domains.registry.inputs import read_cards
from sve_carddb.domains.registry.preview.archive import FrozenJP
from sve_carddb.domains.registry.preview.en_archive import FrozenEN, FrozenRegions
from sve_carddb.domains.registry.preview.review_inputs import (
    RegistryReviewInputs,
    ReviewInputPin,
)
from sve_carddb.domains.registry.records import (
    ArtData,
    ArtUse,
    EnglishPrintingData,
    MappingReviewData,
    Observation,
    PrintingData,
)
from sve_carddb.domains.registry.review import observation
from sve_carddb.domains.registry.transitions.history import History
from sve_carddb.domains.registry.transitions.models import (
    Evidence,
    RegistryBasis,
    Transition,
)
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.parse.pages import official_en, official_jp

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.provenance import Source
    from sve_carddb.domains.registry.storage import RegistryFiles


class Artifact(RecordData):
    batch_id: Text
    inventory_sha256: Hash
    coverage_hash: Hash
    bytes: UInt
    as_of: Date
    filename: Literal["jp.jsonl"] = "jp.jsonl"
    target_region: Literal["jp"] = "jp"
    coverage_scope: Literal[
        "All Japanese official card extractions in the reviewed input batch"
    ] = "All Japanese official card extractions in the reviewed input batch"

    @classmethod
    def from_pin(cls, pin: ReviewInputPin) -> Artifact:
        """Retain verified portable pin fields without a private filesystem root."""
        return cls.model_validate_json(canonical(pin.configuration()))

    def locate(self, root: Path) -> ReviewInputPin:
        """Attach the caller's permanent root only while locating exact bytes."""
        return ReviewInputPin.model_validate_json(
            canonical(
                self.model_dump(exclude={"filename", "target_region", "coverage_scope"})
                | {"root": str(root)}
            )
        )


class RegionalSource(RecordData):
    region: Region
    card_batch: Hash
    parser_version: Text
    image_batch: Hash | None = None


class Configuration(RecordData):
    profile: Literal[
        "registry-confirmed-none-renewal-v1", "registry-art-extension-v1"
    ] = "registry-confirmed-none-renewal-v1"
    registry: RegistryBasis
    review_input: Artifact | None
    sources: tuple[RegionalSource, ...] = Field(min_length=2, max_length=2)
    dependencies: tuple[FilePin, ...] = Field(min_length=1, max_length=1)

    def regions(self) -> tuple[RegionalSource, RegionalSource]:
        """Reject missing, duplicate or reordered source regions."""
        if tuple(source.region for source in self.sources) != ("en", "jp"):
            raise ValueError(
                "Renewal requires explicit EN and JP sources in region order"
            )
        return self.sources[0], self.sources[1]

    def review_pin(self, root: Path) -> ReviewInputPin:
        """Only absence renewal can locate a complete JP coverage artifact."""
        if self.review_input is None:
            raise ValueError("Confirmed-none renewal requires a complete JP artifact")
        return self.review_input.locate(root)


def configuration(record: Transition) -> Configuration:
    """Accept only the supported renewal profile and complete batch pins."""
    try:
        result = Configuration.model_validate_json(
            canonical(parse(record.review_context.context.configuration.encode()))
        )
    except ValueError:
        raise ValueError("Unsupported mapping review renewal configuration") from None
    result.regions()
    if result.registry != record.registry_basis:
        raise ValueError("Renewal context disagrees with registry basis")
    if (
        {source.card_batch for source in result.sources}
        | {
            source.image_batch
            for source in result.sources
            if source.image_batch is not None
        }
    ) != {source.batch_id for source in record.review_context.source_batches}:
        raise ValueError("Renewal source batches disagree with reviewed context")
    return result


def printings(files: RegistryFiles) -> tuple[PrintingData, ...]:
    """Read checked original printing observations without applying any review."""
    return tuple(
        (
            EnglishPrintingData if entry.data["region"] == "en" else PrintingData
        ).model_validate_json(canonical(entry.data))
        for shard in files.shards
        for entry in shard.envelope().records
        if entry.kind == "printing"
    )


@dataclass(frozen=True)
class VerifiedReview:
    evidence: tuple[Evidence, ...]
    uses: tuple[SourceUse, ...]


class ReviewSources:
    def __init__(
        self,
        repo: Path,
        archive: Path,
        store_id: str,
        review_root: Path,
    ) -> None:
        self.history = History(repo)
        self.archive = archive
        self.store_id = store_id
        self.review_root = review_root
        self._providers: dict[tuple[RegionalSource, ...], FrozenRegions] = {}
        self._batches: dict[str, FrozenSources] = {}

    def _batch(self, batch_id: str, *, image: bool = False) -> FrozenSources:
        if batch_id not in self._batches:
            self._batches[batch_id] = FrozenSources(
                self.archive,
                self.store_id,
                batch_id,
                verify_raw=not image,
            )
        return self._batches[batch_id]

    def provider(self, sources: tuple[RegionalSource, ...]) -> FrozenRegions:
        """Reuse immutable source readers within an exact regional batch pair."""
        if sources not in self._providers:
            if tuple(s.region for s in sources) != ("en", "jp"):
                raise ValueError("Renewal requires both frozen source regions")
            en, jp = sources
            self._providers[sources] = FrozenRegions(
                en=FrozenEN(
                    self.archive,
                    self.store_id,
                    en.card_batch,
                    parser_version=en.parser_version,
                    sources=self._batch(en.card_batch),
                ),
                jp=FrozenJP(
                    self.archive,
                    self.store_id,
                    jp.card_batch,
                    parser_version=jp.parser_version,
                    sources=self._batch(jp.card_batch),
                ),
            )
        return self._providers[sources]

    def verify(
        self,
        config: Configuration,
        files: RegistryFiles,
        reviews: tuple[MappingReviewData, ...],
    ) -> VerifiedReview:
        """Match a complete JP artifact and all reviewed EN sources independently."""
        pin = config.review_pin(self.review_root)
        reviewed = RegistryReviewInputs(pin)
        try:
            jp = read_cards(pin.root / pin.batch_id / "jp.jsonl")
        except ValueError:
            raise ValueError("Invalid complete renewal JP card input") from None
        registered = printings(files)
        expected_jp = {p.card_no for p in registered if p.region == "jp"}
        if set(jp) != expected_jp:
            raise ValueError(
                "Renewal JP input must equal the complete registered JP scope"
            )
        provider = self.provider(config.sources)
        uses: list[SourceUse] = []
        coverage: dict[str, Evidence] = {}
        for number, card in sorted(jp.items()):
            found = provider.card("jp", number)
            expected = Observation.model_validate_json(
                canonical(observation(card, "jp"))
            )
            if found is None or found.observation != expected:
                raise ValueError(
                    "Renewal JP input differs from frozen source observation"
                )
            uses.append(
                SourceUse(
                    source=found.source,
                    usage="identity_transition_coverage",
                    locator=number,
                )
            )
            coverage[number] = Evidence(
                batch_id=found.source.archive.batch_id,
                source_version_id=found.source.id,
                locator="jp:all",
                role="mapping_coverage",
            )
        evidence: list[Evidence] = [coverage[min(coverage)]]
        for review in reviews:
            if not reviewed.coverage(review):
                raise ValueError(
                    "Renewal after review does not match complete input pin"
                )
            expected_en = {
                p.observation for p in registered if p.card_id == review.card_id
            }
            if (
                any(p.region != "en" for p in registered if p.card_id == review.card_id)
                or set(review.observations) != expected_en
                or len(review.observations) != len(expected_en)
            ):
                raise ValueError(
                    "Renewal must review all and only the card's EN printings"
                )
            for expected in review.observations:
                found = provider.card("en", expected.card_no)
                if found is None or found.observation != expected:
                    raise ValueError(
                        "Renewal EN observation differs from frozen source"
                    )
                uses.append(
                    SourceUse(
                        source=found.source,
                        usage="identity_transition_observation",
                        locator=expected.card_no,
                    )
                )
                evidence.append(
                    Evidence(
                        batch_id=found.source.archive.batch_id,
                        source_version_id=found.source.id,
                        locator=expected.card_no,
                        role="identity_observation",
                    )
                )
        return VerifiedReview(
            tuple(
                sorted(
                    set(evidence), key=lambda e: canonical(e.model_dump(mode="json"))
                )
            ),
            uses_sorted(uses),
        )

    def _art_use(
        self,
        art: ArtData,
        use: ArtUse,
        printing: PrintingData,
        config: Configuration,
    ) -> VerifiedReview:
        mapping = next(
            (m for m in printing.source_face_map if m.face_id == use.face_id), None
        )
        if (
            printing.card_id != art.card_id
            or use.face_id != art.face_id
            or mapping is None
        ):
            raise ValueError("Art extension cannot cross card or face ownership")
        provider = self.provider(config.sources)
        found = provider.card(printing.region, printing.card_no)
        if found is None or found.observation != printing.observation:
            raise ValueError(
                "Art extension printing observation differs from frozen source"
            )
        source_pin = next(s for s in config.sources if s.region == printing.region)
        if source_pin.image_batch is None:
            raise ValueError("Art extension requires frozen image batch evidence")
        image = self._image(printing, source_pin, found.source.id, mapping.source_index)
        locator = canonical(
            {"art_id": art.id, "printing_id": printing.id, "face_id": use.face_id}
        ).decode()
        return VerifiedReview(
            tuple(
                Evidence(
                    batch_id=source.archive.batch_id,
                    source_version_id=source.id,
                    locator=locator,
                    role=role,
                )
                for source, role in (
                    (found.source, "art_observation"),
                    (image, "art_image"),
                )
            ),
            tuple(
                SourceUse(source=source, usage=role, locator=locator)
                for source, role in (
                    (found.source, "identity_transition_art"),
                    (image, "identity_transition_art_image"),
                )
            ),
        )

    def _image(
        self,
        printing: PrintingData,
        source_pin: RegionalSource,
        version: str,
        source_index: int,
    ) -> Source:
        assert source_pin.image_batch is not None
        page = physical_sources.page(
            self._batch(source_pin.card_batch),
            version,
            region=printing.region,
            parser_version=source_pin.parser_version,
        ).record.get()
        if source_index >= len(page.faces):
            raise ValueError("Art extension source face is unavailable")
        adapter = official_jp if printing.region == "jp" else official_en
        url = official_jp.image_url(page.faces[source_index].image, adapter.CARD_DIR)
        images = self._batch(source_pin.image_batch, image=True)
        image_version = next(
            (v.source_version_id for v in images.current_versions if v.url == url), None
        )
        if image_version is None:
            raise ValueError("Art extension image is absent from the frozen batch")
        image, _raw, descriptor = images.read(
            image_version, parser_version="registry-art-image-v1"
        )
        if (descriptor.provider, descriptor.kind, image.kind) != (
            printing.region,
            "image",
            "image",
        ):
            raise ValueError("Art extension image source identity mismatch")
        return image

    def verify_art(
        self,
        config: Configuration,
        files: RegistryFiles,
        arts: tuple[ArtData, ...],
    ) -> VerifiedReview:
        """Pin the complete manually reviewed use collection and every original image."""
        registered = {p.id: p for p in printings(files)}
        evidence: list[Evidence] = []
        uses: list[SourceUse] = []
        for art in arts:
            for use in art.uses:
                if use.printing_id not in registered:
                    raise ValueError("Art extension names an unknown printing")
                verified = self._art_use(art, use, registered[use.printing_id], config)
                evidence.extend(verified.evidence)
                uses.extend(verified.uses)
        return VerifiedReview(
            tuple(
                sorted(
                    set(evidence), key=lambda e: canonical(e.model_dump(mode="json"))
                )
            ),
            uses_sorted(uses),
        )

    def transition(
        self,
        record: Transition,
        files: RegistryFiles,
        reviews: tuple[MappingReviewData | ArtData, ...],
    ) -> VerifiedReview:
        """Reconstruct historical evidence rather than trusting authored hashes."""
        config = configuration(record)
        if config.dependencies != (
            self.history.dependency(record.review_context.context.program_revision),
        ):
            raise ValueError("Renewal dependency pin differs from historical Git bytes")
        if config.profile == "registry-art-extension-v1":
            if any(not isinstance(data, ArtData) for data in reviews):
                raise ValueError("Art extension context cannot approve mapping reviews")
            verified = self.verify_art(
                config,
                files,
                tuple(data for data in reviews if isinstance(data, ArtData)),
            )
        else:
            if any(not isinstance(data, MappingReviewData) for data in reviews):
                raise ValueError("Absence review context cannot approve art extensions")
            verified = self.verify(
                config,
                files,
                tuple(data for data in reviews if isinstance(data, MappingReviewData)),
            )
        if verified.evidence != record.evidence:
            raise ValueError(
                "Renewal evidence differs from reconstructed frozen inputs"
            )
        return verified
