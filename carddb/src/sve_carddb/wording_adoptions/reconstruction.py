"""Replay pinned authored inventories and every historical corrected observation."""

import sys
import tempfile
import unicodedata
from dataclasses import dataclass
from datetime import date
from pathlib import Path, PurePosixPath
from types import MappingProxyType
from typing import TYPE_CHECKING, Literal, Self

from pydantic import model_validator

from sve_carddb.build_inputs import Revision, SourceUse, uses_sorted
from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.products.loader import load_products
from sve_carddb.products.models import Evidence
from sve_carddb.registry.records import (
    CorrectionData,
    CorrectionEvidence,
    Hash,
    RecordData,
)
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import canonical, digest, object_value
from sve_carddb.source_corrections.images import evidence_url
from sve_carddb.source_corrections.plan import (
    corrected_observations,
    historical_application,
)
from sve_carddb.text_observations.archive import FrozenTexts
from sve_carddb.wording_adoptions.models import Correction, Observation, ReviewContext
from sve_carddb.wording_adoptions.scope import rebuild_raw_scope

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build_inputs import Source
    from sve_carddb.products.loader import ProductSnapshot
    from sve_carddb.registry.snapshot import RegistrySnapshot
    from sve_carddb.source_corrections.plan import Application
    from sve_carddb.text_observations.models import FaceObservation


class AuthoredPin(RecordData):
    authored_revision: Revision
    index_path: str
    index_hash: Hash


class Configuration(RecordData):
    recipe: Literal["wording-review-v1"]
    registry: AuthoredPin
    products: AuthoredPin | None
    product_identity: Literal["disabled"]
    corrections: Literal["registry-active-v1"]
    errata: Literal["disabled"]
    errata_as_of: str
    regions: tuple[Literal["jp", "en"], ...]
    parser: Literal["text-observations-v1"]
    projection: Literal["effect-presence-v1-then-source-correction-v1"]
    python_version: str
    unicode_version: str

    @model_validator(mode="after")
    def _scope(self) -> Self:
        if (
            self.python_version != sys.version.split()[0]
            or self.unicode_version != unicodedata.unidata_version
        ):
            raise ValueError("Pinned Python/Unicode runtime cannot be replayed")
        if date.fromisoformat(self.errata_as_of).isoformat() != self.errata_as_of:
            raise ValueError("Review errata cutoff must be a complete ISO date")
        if self.regions not in {("jp",), ("en",), ("en", "jp")}:
            raise ValueError("Review regions must be sorted, unique and nonempty")
        if self.registry.index_path != "authored/ids/index.yaml" or (
            self.products is not None
            and self.products.index_path != "authored/products/index.yaml"
        ):
            raise ValueError("Review authored entry path does not match its capability")
        return self


@dataclass(frozen=True)
class ReconstructedScope:
    registry: RegistrySnapshot
    products: ProductSnapshot | None
    observations: tuple[Observation, ...]
    contents: Mapping[str, FaceObservation]
    uses: tuple[SourceUse, ...]
    dependencies: Mapping[str, bytes]
    applications: tuple[Application, ...] = ()


def runtime_dependencies(
    repository: PinnedRepository, revision: str
) -> dict[str, bytes]:
    """Verify the producing runtime as well as the independently pinned reviewed runtime."""
    runtime = Path(__file__).resolve().parents[4]
    names = {"carddb/uv.lock", "carddb/pyproject.toml"} | {
        path.relative_to(runtime).as_posix()
        for path in (runtime / "carddb/src/sve_carddb").rglob("*.py")
        if not path.is_symlink()
    }
    files = repository.read_many(revision, tuple(sorted(names)))
    if any((runtime / name).read_bytes() != content for name, content in files.items()):
        raise ValueError("Historical wording runtime cannot be replayed")
    return files


class Reconstructor:
    def __init__(self, root: Path, stores: Mapping[str, Path]) -> None:
        self.repository = PinnedRepository(root)
        self.stores = dict(stores)
        self.contexts: dict[
            bytes, tuple[RegistrySnapshot, ProductSnapshot | None, dict[str, bytes]]
        ] = {}
        self.batches: dict[tuple[str, str], FrozenSources] = {}
        self.providers: dict[tuple[str, str, str], FrozenTexts] = {}
        self.scopes: dict[tuple[bytes, str, str], ReconstructedScope] = {}

    def batch(self, store: str, batch: str) -> FrozenSources:
        """Verify a sealed inventory before resolving any caller-supplied source."""
        key = store, batch
        if key not in self.batches:
            root = self.stores.get(store)
            if root is None:
                raise ValueError("Reviewed archive store is not configured")
            self.batches[key] = FrozenSources(root, store, batch)
        return self.batches[key]

    def _inventory(self, root: Path, pin: AuthoredPin) -> dict[str, bytes]:
        content = self.repository.read(pin.authored_revision, pin.index_path)
        relative = PurePosixPath(pin.index_path).relative_to("authored")
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
        raw = read_yaml(target)
        if digest(canonical(raw)) != pin.index_hash:
            raise ValueError("Reviewed authored index hash mismatch")
        files = {pin.index_path: content}
        for name in object_value(object_value(raw).get("includes")):
            path = PurePosixPath(name)
            if path.is_absolute() or ".." in path.parts or path.as_posix() != name:
                raise ValueError("Unsafe reviewed authored shard path")
            content = self.repository.read(pin.authored_revision, "authored/" + name)
            target = root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
            files["authored/" + name] = content
        return files

    def _context(
        self, review: ReviewContext
    ) -> tuple[RegistrySnapshot, ProductSnapshot | None, dict[str, bytes]]:
        key = canonical(review.context.model_dump(mode="json"))
        if key not in self.contexts:
            config = Configuration.model_validate_json(review.context.configuration)
            files = runtime_dependencies(
                self.repository, review.context.program_revision
            )
            with tempfile.TemporaryDirectory(prefix="wording-review-") as name:
                root = Path(name)
                files.update(self._inventory(root, config.registry))
                registry = load_registry(root)
                products = None
                if config.products is not None:
                    files.update(self._inventory(root, config.products))
                    products = load_products(root, registry=registry)
                declared = {p.name: p.sha256 for p in review.context.dependencies}
                if declared != {name: digest(raw) for name, raw in files.items()}:
                    raise ValueError(
                        "Reviewed dependency closure is incomplete or changed"
                    )
                self.contexts[key] = registry, products, files
        return self.contexts[key]

    def scope(
        self, review: ReviewContext, face: str, region: Literal["jp", "en"]
    ) -> ReconstructedScope:
        """Rebuild every version before corrections, content hashes or checked keys."""
        key = canonical(review.model_dump(mode="json")), face, region
        if key in self.scopes:
            return self.scopes[key]
        config = Configuration.model_validate_json(review.context.configuration)
        if region not in config.regions:
            raise ValueError("Adoption region is outside the reviewed scope")
        registry, products, files = self._context(review)
        providers = []
        for pin in review.source_batches:
            self.batch(pin.store_id, pin.batch_id)
            provider_key = pin.store_id, pin.batch_id, region
            if provider_key not in self.providers:
                self.providers[provider_key] = FrozenTexts(
                    self.stores[pin.store_id],
                    pin.store_id,
                    pin.batch_id,
                    region=region,
                    parser_version=config.parser,
                )
            providers.append(self.providers[provider_key])
        raw = rebuild_raw_scope(registry, face, region, tuple(providers))
        uses = list(raw.uses)
        observations = []
        contents = {}
        applications: list[Application] = []
        for item in raw.observations:
            applications.extend(self.applications(registry, review, item))
            observation, corrected, correction_uses = self.rebuild_observation(
                registry, review, item
            )
            uses.extend(correction_uses)
            observations.append(observation)
            contents[observation.observation_key] = corrected
        self.scopes[key] = ReconstructedScope(
            registry,
            products,
            tuple(sorted(observations, key=lambda o: o.observation_key)),
            MappingProxyType(contents),
            uses_sorted(uses),
            MappingProxyType(files),
            tuple(applications),
        )
        return self.scopes[key]

    def rebuild_observation(
        self, registry: RegistrySnapshot, review: ReviewContext, item: FaceObservation
    ) -> tuple[Observation, FaceObservation, tuple[SourceUse, ...]]:
        """Project absence first and bind every applicable correction independently."""
        if item.card.has_errata_link:
            raise ValueError("Unimplemented errata coverage cannot confirm wording")
        applications = self.applications(registry, review, item)
        corrected = corrected_observations((item,), applications)[0]
        uses = tuple(use for a in applications for use in a.uses())
        observation = Observation(
            observation_key="pending",
            printing_id=item.printing_id,
            source_index=item.source_index,
            source_version_id=item.card.source.id,
            raw_hash=item.card.source.sha256,
            parser_version=item.card.source.parser_version,
            raw_face_hash=item.card.faces[item.source_index].fingerprint(),
            effect_presence=item.card.effect_presence[item.source_index],
            corrections=tuple(
                Correction(
                    correction_id=a.data.id,
                    record_hash=a.key(),
                    decision_id=a.record.decision_id or "",
                    status=a.status,
                )
                for a in applications
                if a.status in {"applied", "already_fixed"}
            ),
            content_hash=corrected.content.fingerprint(),
        )
        observation = observation.model_copy(
            update={"observation_key": observation.key(item.face_id, item.region)}
        )
        return observation, corrected, uses

    def applications(
        self, registry: RegistrySnapshot, review: ReviewContext, item: FaceObservation
    ) -> tuple[Application, ...]:
        """Revalidate active decisions for each historical raw version separately."""
        records = sorted(
            (
                r
                for r in registry.records.values()
                if isinstance(r.data, CorrectionData)
                and r.data.state == "active"
                and (r.data.printing_id, r.data.face_id)
                == (item.printing_id, item.face_id)
            ),
            key=lambda r: r.record_key,
        )
        applications = tuple(
            historical_application(registry, r, item, _Images(self, review))
            for r in records
        )
        fields = [a.data.field for a in applications]
        if len(fields) != len(set(fields)) or any(
            a.status not in {"applied", "already_fixed"} for a in applications
        ):
            raise ValueError(
                "Active historical correction is conflicting or unconfirmed"
            )
        return applications


class _Images:
    def __init__(self, reconstruction: Reconstructor, review: ReviewContext) -> None:
        self.reconstruction = reconstruction
        self.review = review

    def image(self, evidence: CorrectionEvidence) -> Source:
        """An image outside declared review batches cannot satisfy a correction."""
        found = []
        for pin in self.review.source_batches:
            batch = self.reconstruction.batch(pin.store_id, pin.batch_id)
            for entry in batch.inventory.entries:
                descriptor = batch.descriptor(entry.source_version_id)
                if (
                    descriptor.provider,
                    descriptor.kind,
                    descriptor.url,
                    descriptor.raw_sha256,
                ) == (
                    evidence.region,
                    "image",
                    evidence_url(evidence),
                    evidence.sha256,
                ):
                    found.append(
                        batch.read(
                            descriptor.id, parser_version="correction-image-evidence-v1"
                        )[0]
                    )
        if not found or any(s.values() != found[0].values() for s in found):
            raise ValueError(
                "Historical correction image closure is absent or ambiguous"
            )
        return found[0]


def scope_evidence(scope: ReconstructedScope) -> tuple[Evidence, ...]:
    """Provide the complete independently recomputed frozen evidence membership."""
    return tuple(
        sorted(
            {
                Evidence(
                    store_id=u.source.archive.store_id,
                    batch_id=u.source.archive.batch_id,
                    source_version_id=u.source.id,
                    locator=u.locator,
                    role=u.usage,
                )
                for u in scope.uses
            },
            key=lambda e: canonical(e.model_dump(mode="json")),
        )
    )
